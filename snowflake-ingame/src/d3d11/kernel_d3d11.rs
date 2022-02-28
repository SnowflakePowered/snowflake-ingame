use crate::d3d11::hook_d3d11::FnPresentHook;
use crate::d3d11::overlay_d3d11::D3D11Overlay;
use crate::hook::HookChain;
use crate::ipc::cmd::{GameWindowCommandType, Dimensions};
use crate::ipc::IpcHandle;
use crate::{Direct3D11HookContext, GameWindowCommand};
use std::borrow::{Borrow, BorrowMut};
use std::cell::RefCell;
use std::error::Error;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use windows::Win32::Graphics::Direct3D11::{ID3D11Device1, ID3D11Texture2D, D3D11_TEXTURE2D_DESC};
use windows::Win32::Graphics::Dxgi::*;

use imgui::{Context, Image, Window};
use tokio::io::AsyncWriteExt;

/// Kernel for a D3D11 hook.
///
/// `overlay` and `imgui` should never be accessed outside of the Direct3D11 rendering thread.
/// The mutexes to which should only ever be acquired inside an `FnPresentHook` or `FnResizeBuffersHook`,
/// to ensure the soundness of the Send and Sync impls for
pub struct Direct3D11Kernel {
    hook: Direct3D11HookContext,
    overlay: Pin<Arc<RwLock<D3D11Overlay>>>,
    imgui: Pin<Arc<RwLock<D3D11ImguiController>>>,
    ipc: IpcHandle,
}

pub struct D3D11ImguiController {
    imgui: Context,
}

impl D3D11ImguiController {
    pub fn frame<F: FnOnce(&mut Context, &mut D3D11Overlay)>(&mut self, overlay: &mut D3D11Overlay, f: F) {
        f(&mut self.imgui, overlay)
    }
}

unsafe impl Send for D3D11ImguiController {}
unsafe impl Sync for D3D11ImguiController {}

impl Direct3D11Kernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        Ok(Direct3D11Kernel {
            hook: Direct3D11HookContext::init()?,
            overlay: Pin::new(Arc::new(RwLock::new(D3D11Overlay::new()))),
            imgui: Pin::new(Arc::new(RwLock::new(D3D11ImguiController {
                imgui: Context::create(),
            }))),
            ipc,
        })
    }

    fn make_present(&self) -> FnPresentHook {
        let handle = self.ipc.clone();
        let overlay = self.overlay.clone();
        let imgui = self.imgui.clone();
        Box::new(
            move |this: IDXGISwapChain, sync: u32, flags: u32, mut next| {
                let handle = handle.clone();
                let overlay = overlay.clone();
                let imgui = imgui.clone();
                (|| unsafe {
                    let mut overlay = overlay.write()?;
                    let mut imgui = imgui.write()?;

                    // Handle update of any overlay here.
                    if let Ok(cmd) = handle.try_recv() {
                        match &cmd.ty {
                            &GameWindowCommandType::OVERLAY => {
                                overlay.refresh(unsafe { cmd.params.overlay_event });
                            }
                            _ => {}
                        }
                    }

                    let swapchain_desc = this.GetDesc()?;
                    let backbuffer = this.GetBuffer::<ID3D11Texture2D>(0)?;

                    let mut backbuffer_desc: D3D11_TEXTURE2D_DESC = Default::default();
                    backbuffer.GetDesc(&mut backbuffer_desc);

                    let size = Dimensions::new(backbuffer_desc.Width, backbuffer_desc.Height);
                    if !overlay.size_matches_viewpoint(&size) {
                        handle.send(GameWindowCommand::window_resize(&size))?;
                    }

                    if !overlay.ready_to_initialize() {
                        eprintln!("[dx11] Texture handle not ready");
                        return Ok::<_, Box<dyn Error>>(());
                    }

                    let device = this.GetDevice::<ID3D11Device1>()?;

                    if !overlay.prepare_paint(device, swapchain_desc.OutputWindow) {
                        eprintln!("[dx11] Failed to refresh texture for output window");
                        return Ok::<_, Box<dyn Error>>(());
                    }

                    // imgui stuff here.
                    // We don't need an external mutex here because the overlay will not change underneath us,
                    // since overlay is updated within Present now.
                    if overlay.acquire_sync() {
                        imgui.frame(&mut overlay, |ctx, overlay| {
                           let ui = ctx.frame();
                            ui.show_demo_window(&mut false);
                            let draw = ui.render();

                        });
                        overlay.release_sync();
                    }

                    Ok::<_, Box<dyn Error>>(())
                })()
                .unwrap_or(());
                let fp = next.fp_next();
                fp(this, sync, flags, next)
            },
        )
    }

    pub fn init(&mut self) -> Result<(), Box<dyn Error>> {
        self.hook.new(
            self.make_present(),
            |this, bufc, w, h, format, flags, mut next| {
                let fp = next.fp_next();
                fp(this, bufc, w, h, format, flags, next)
            },
        )?;

        Ok(())
    }
}
