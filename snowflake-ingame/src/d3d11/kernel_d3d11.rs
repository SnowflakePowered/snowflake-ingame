use std::cell::RefCell;
use std::error::Error;
use std::sync::Arc;
use windows::Win32::Graphics::Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::*;
use crate::d3d11::overlay_d3d11::D3D11Overlay;
use crate::{Direct3D11HookContext, GameWindowCommand};
use crate::ipc::IpcHandle;
use crate::hook::HookChain;
use crate::ipc::cmd::{GameWindowCommandType, Size};

pub struct Direct3D11Kernel {
    hook: Direct3D11HookContext,
    overlay: Arc<RefCell<D3D11Overlay>>,
    ipc: IpcHandle
}

impl Direct3D11Kernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        Ok(Direct3D11Kernel {
            hook: Direct3D11HookContext::init()?,
            overlay: Arc::new(RefCell::new(D3D11Overlay::new())),
            ipc
        })
    }

    pub fn init(&mut self) -> Result<(), Box<dyn Error>>{
        let handle = self.ipc.clone();
        let overlay = self.overlay.clone();
        self.hook.new(
            Box::new(|this: IDXGISwapChain, sync: u32, flags: u32, mut next| {
                // let mut overlay = overlay.borrow_mut();
                // if let Ok(cmd) = handle.try_recv() {
                //     match &cmd.ty {
                //         &GameWindowCommandType::OVERLAY => {
                //             overlay.refresh(unsafe { cmd.params.overlay_event }) ;
                //         },
                //         _ => {}
                //     }
                // }


                (|| unsafe {
                    let swapchain_desc = this.GetDesc().unwrap();
                    let backbuffer = this.GetBuffer::<ID3D11Texture2D>(0)?;

                    let mut backbuffer_desc: D3D11_TEXTURE2D_DESC = Default::default();
                    backbuffer.GetDesc(&mut backbuffer_desc);

                    let size = Size::new(backbuffer_desc.Width, backbuffer_desc.Height);
                    if !overlay.borrow().size_matches_viewpoint(&size) {
                        handle.send(GameWindowCommand::window_resize(&size));
                    }
                    Ok::<_, Box<dyn Error>>(())
                })().unwrap_or(());
                    let fp = next.fp_next();
                    fp(this, sync, flags, next)
              }),

            |this, bufc, w, h, format, flags, mut next| {
                let fp = next.fp_next();
                fp(this, bufc, w, h, format, flags, next)
            }
        )?;

        Ok(())
    }
}