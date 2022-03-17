use std::error::Error;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use parking_lot::{RwLock, RwLockWriteGuard};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_TEXTURE2D_DESC, ID3D11Device1, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::*;
use imgui_renderer_dx11::RenderToken;

use crate::common::{OverlayWindow, RenderError};
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
/// to ensure the soundness of the Send and Sync impls for Direct3D11Overlay
pub struct Direct3D11Kernel {
    hook: Direct3D11HookContext,
    overlay: Arc<RwLock<Direct3D11Overlay>>,
    imgui: Arc<RwLock<Direct3D11ImguiController>>,
    ipc: IpcHandle,
}

impl Direct3D11Kernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        Ok(Direct3D11Kernel {
            hook: Direct3D11HookContext::init()?,
            overlay: Arc::new(RwLock::new(Direct3D11Overlay::new())),
            imgui: Arc::new(RwLock::new(Direct3D11ImguiController::new())),
            ipc,
        })
    }

    fn present_impl(
        handle: IpcHandle,
        mut overlay: RwLockWriteGuard<Direct3D11Overlay>,
        mut imgui: RwLockWriteGuard<Direct3D11ImguiController>,
        this: &IDXGISwapChain,
    ) -> Result<RenderToken, RenderError> {
        // Handle update of any overlay here.
        if let Ok(cmd) = handle.try_recv() {
            match &cmd.ty {
                &GameWindowCommandType::OVERLAY => {
                    eprintln!("[dx11] received overlay texture event");
                    overlay.refresh( unsafe { cmd.params.overlay_event })
                        .unwrap_or_else(|e| eprintln!("[dx11] handle error: {}", e));
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
            return Err(RenderError::OverlayHandleNotReady)
        }

        let device = unsafe { this.GetDevice::<ID3D11Device1>()? };

        overlay.prepare_paint(device, swapchain_desc.OutputWindow)
            .map_err(|e| RenderError::OverlayPaintNotReady(Box::new(e)))?;

        imgui.prepare_paint(&this, size)
            .map_err(|e| RenderError::ImGuiNotReady(Box::new(e)))?;

        // imgui stuff here.
        // We don't need an external mutex here because the overlay will not change underneath us,
        // since overlay is updated within Present now.
        if let Some(_kmt) = overlay.acquire_sync() {
            imgui.frame(&mut overlay, |ctx, render, overlay| {
                let ui = ctx.frame();
                overlay.paint(|tid, dim|  OverlayWindow::new(&ui, tid, dim));
                ui.show_demo_window(&mut false);
                ui.show_metrics_window(&mut false);
                render.render(ui.render())
            })
        } else {
            Err(RenderError::OverlayMutexNotReady)
        }
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
                let handle = handle.clone();
                match Direct3D11Kernel::present_impl(handle, overlay.write(),
                                               imgui.write(), &this)
                {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[dx11] {}", e)
                    }
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
                Direct3D11Kernel::resize_impl(imgui.write());
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
