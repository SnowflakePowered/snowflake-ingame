use std::error::Error;
use std::mem::ManuallyDrop;
use std::pin::Pin;
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use windows::Win32::Graphics::Direct3D11::{
    D3D11_TEXTURE2D_DESC, ID3D11Device1, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::*;

use crate::common::OverlayWindow;
use crate::d3d11::hook::{Direct3D11HookContext, FnPresentHook, FnResizeBuffersHook};
use crate::d3d11::imgui::Direct3D11ImguiController;
use crate::d3d11::overlay::Direct3D11Overlay;
use crate::hook::{HookChain, HookHandle};
use crate::ipc::cmd::{GameWindowCommand, GameWindowCommandType};
use crate::ipc::IpcHandle;

/// Kernel for a D3D11 hook.
///
/// `overlay` and `imgui` should never be accessed outside of the Direct3D11 rendering thread.
/// The mutexes to which should only ever be acquired inside an `FnPresentHook` or `FnResizeBuffersHook`,
/// to ensure the soundness of the Send and Sync impls for
pub struct Direct3D11Kernel {
    hook: Direct3D11HookContext,
    overlay: Pin<Arc<RwLock<Direct3D11Overlay>>>,
    imgui: Pin<Arc<RwLock<Direct3D11ImguiController>>>,
    ipc: IpcHandle,
}

impl Direct3D11Kernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        Ok(Direct3D11Kernel {
            hook: Direct3D11HookContext::init()?,
            overlay: Pin::new(Arc::new(RwLock::new(Direct3D11Overlay::new()))),
            imgui: Pin::new(Arc::new(RwLock::new(Direct3D11ImguiController::new()))),
            ipc,
        })
    }

    fn present_impl(
        handle: IpcHandle,
        mut overlay: RwLockWriteGuard<Direct3D11Overlay>,
        mut imgui: RwLockWriteGuard<Direct3D11ImguiController>,
        this: &IDXGISwapChain,
    ) -> Result<(), Box<dyn Error>> {
        // Handle update of any overlay here.
        if let Ok(cmd) = handle.try_recv() {
            match &cmd.ty {
                &GameWindowCommandType::OVERLAY => {
                    eprintln!("[dx11] received overlay texture event");
                    overlay.refresh( unsafe { cmd.params.overlay_event });
                }
                _ => {}
            }
        }

        let swapchain_desc = unsafe { this.GetDesc()? };
        let backbuffer = unsafe { this.GetBuffer::<ID3D11Texture2D>(0)? };

        let backbuffer_desc: D3D11_TEXTURE2D_DESC  = unsafe {
            let mut backbuffer_desc = Default::default();
            backbuffer.GetDesc(&mut backbuffer_desc);
            backbuffer_desc
        };


        let size = backbuffer_desc.into();
        if !overlay.size_matches_viewpoint(&size) {
            handle.send(GameWindowCommand::window_resize(&size))?;
        }

        if !overlay.ready_to_initialize() {
            eprintln!("[dx11] Texture handle not ready");
            return Ok::<_, Box<dyn Error>>(());
        }

        let device = unsafe { this.GetDevice::<ID3D11Device1>()? };

        if !overlay.prepare_paint(device, swapchain_desc.OutputWindow) {
            eprintln!("[dx11] Failed to refresh texture for output window");
            return Ok::<_, Box<dyn Error>>(());
        }

        if !imgui.prepare_paint(&this, size) {
            eprintln!("[dx11] Failed to setup imgui render state");
            return Ok::<_, Box<dyn Error>>(());
        }

        // imgui stuff here.
        // We don't need an external mutex here because the overlay will not change underneath us,
        // since overlay is updated within Present now.
        if overlay.acquire_sync() {
            imgui.frame(&mut overlay, |ctx, render, overlay| {
                let ui = ctx.frame();
                overlay.paint(|tid, dim|  OverlayWindow::new(&ui, tid, dim));
                ui.show_demo_window(&mut false);
                render.render(ui.render()).unwrap()
            });

            overlay.release_sync();
        }

        Ok::<_, Box<dyn Error>>(())
    }

    fn resize_impl(mut imgui: RwLockWriteGuard<Direct3D11ImguiController>) {
        imgui.invalidate_rtv();
    }

    fn make_present(&self) -> FnPresentHook {
        let handle = self.ipc.clone();
        let overlay = self.overlay.clone();
        let imgui = self.imgui.clone();
        Box::new(
            move |this: IDXGISwapChain, sync: u32, flags: u32, mut next| {
                if let (Ok(overlay), Ok(imgui)) = (overlay.write(), imgui.write()) {
                    let handle = handle.clone();
                    Direct3D11Kernel::present_impl(handle, overlay, imgui, &this).unwrap_or(());
                } else {
                    eprintln!("[dx11] unable to acquire overlay write guards")
                }

                let fp = next.fp_next();
                fp(this, sync, flags, next)
            },
        )
    }

    fn make_resize(&self) -> FnResizeBuffersHook {
        let imgui = self.imgui.clone();
        Box::new(
            move |this: IDXGISwapChain, buf_cnt, width, height, format, flags, mut next| {
                if let Ok(imgui) = imgui.write() {
                    Direct3D11Kernel::resize_impl(imgui);
                }

                let fp = next.fp_next();
                fp(this, buf_cnt, width, height, format, flags, next)
            },
        )
    }

    pub fn init(&mut self) -> Result<ManuallyDrop<impl HookHandle>, Box<dyn Error>> {
        println!("[dx11] init");
        let handle = self.hook
            .new(self.make_present(), self.make_resize())?
            .persist();

        Ok(handle)
    }
}
