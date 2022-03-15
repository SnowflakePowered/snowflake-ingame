use std::borrow::Borrow;
use std::error::Error;
use std::mem::MaybeUninit;
use std::pin::Pin;
use std::ptr;
use std::sync::{Arc, RwLock, RwLockWriteGuard};

use imgui::{Condition, Context, DrawData, Image, StyleVar, TextureId, Window, WindowFlags};
use tokio::io::AsyncWriteExt;
use windows::core::Result as HResult;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11Device1, ID3D11RenderTargetView, ID3D11Texture2D, D3D11_TEXTURE2D_DESC,
};
use windows::Win32::Graphics::Dxgi::*;

use imgui_renderer_dx11::{Direct3D11ImguiRenderer, RenderToken};

use crate::common::Dimensions;
use crate::d3d11::hook_d3d11::{FnPresentHook, FnResizeBuffersHook};
use crate::d3d11::overlay_d3d11::D3D11Overlay;
use crate::hook::HookChain;
use crate::ipc::cmd::GameWindowCommandType;
use crate::ipc::IpcHandle;
use crate::{Direct3D11HookContext, GameWindowCommand, HookHandle};

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
    renderer: Option<Direct3D11ImguiRenderer>,
    device: Option<ID3D11Device>,
    window: HWND,
    rtv: Option<ID3D11RenderTargetView>,
}

pub struct Render<'a> {
    render: Option<&'a mut Direct3D11ImguiRenderer>,
    rtv: Option<&'a ID3D11RenderTargetView>,
}

impl Render<'_> {
    pub fn render(mut self, draw_data: &DrawData) -> HResult<()> {
        if let Some(renderer) = self.render {
            renderer.render(draw_data)?;
        }
        Ok(())
    }
}

impl D3D11ImguiController {
    pub fn new() -> D3D11ImguiController {
        D3D11ImguiController {
            imgui: Context::create(),
            renderer: None,
            device: None,
            window: HWND(0),
            rtv: None,
        }
    }

    pub const fn renderer_ready(&self) -> bool {
        self.renderer.is_some() && self.device.is_some()
    }

    pub const fn rtv_ready(&self) -> bool {
        self.rtv.is_some()
    }

    // todo: use RenderToken POW
    pub fn frame<'a, F: FnOnce(&mut Context, Render, &mut D3D11Overlay) -> ()>(
        &mut self,
        overlay: &mut D3D11Overlay,
        f: F,
    ) {
        let renderer = Render {
            render: self.renderer.as_mut(),
            rtv: self.rtv.as_ref(),
        };
        f(&mut self.imgui, renderer, overlay);
    }

    unsafe fn init_renderer(&mut self, swapchain: &IDXGISwapChain, window: HWND) -> HResult<()> {
        let device = swapchain.GetDevice()?;
        self.renderer = Some(Direct3D11ImguiRenderer::new(&device, &mut self.imgui)?);
        self.device = Some(device);
        self.window = window;
        Ok(())
    }

    pub fn invalidate_renderer(&mut self) {
        self.renderer = None;
        self.device = None;
    }

    pub fn invalidate_rtv(&mut self) {
        self.rtv = None;
    }

    unsafe fn init_rtv(&mut self, swapchain: &IDXGISwapChain) -> HResult<()> {
        let device: ID3D11Device = swapchain.GetDevice()?;
        let context = {
            let mut context = MaybeUninit::uninit();
            device.GetImmediateContext(context.as_mut_ptr());
            context.assume_init()
        };

        let mut rtv = [None];
        if let Some(context) = &context {
            context.OMGetRenderTargets(&mut rtv, ptr::null_mut());
        }

        if let Some(Some(rtv)) = rtv.into_iter().next() {
            self.rtv = Some(rtv)
        } else {
            let back_buffer: ID3D11Texture2D = swapchain.GetBuffer(0)?;
            let rtv = device.CreateRenderTargetView(back_buffer, std::ptr::null())?;
            if let Some(context) = &context {
                context.OMSetRenderTargets(&[Some(rtv.clone())], None);
            }
            self.rtv = Some(rtv);
        }
        Ok(())
    }

    pub fn prepare_paint(&mut self, swapchain: &IDXGISwapChain, screen_dim: Dimensions) -> bool {
        let swap_desc: DXGI_SWAP_CHAIN_DESC = if let Ok(swap_desc) = unsafe { swapchain.GetDesc() }
        {
            swap_desc
        } else {
            eprintln!("[dx11] unable to get swapchain desc");
            return false;
        };

        if swap_desc.OutputWindow != self.window {
            self.invalidate_renderer();
            self.invalidate_rtv();
        }

        if !self.renderer_ready() {
            if let Err(_) = unsafe { self.init_renderer(swapchain, swap_desc.OutputWindow) } {
                eprintln!("[dx11] unable to initialize renderer");
                return false;
            }
        }

        // todo: fix rtv reset
        if !self.rtv_ready() {
            if let Err(_) = unsafe { self.init_rtv(&swapchain) } {
                eprintln!("[dx11] unable to set render target view");
                return false;
            }
        }

        // set screen size..
        self.imgui.io_mut().display_size = screen_dim.into();
        self.window = swap_desc.OutputWindow;
        true
    }
}

unsafe impl Send for D3D11ImguiController {}
unsafe impl Sync for D3D11ImguiController {}

impl Direct3D11Kernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        Ok(Direct3D11Kernel {
            hook: Direct3D11HookContext::init()?,
            overlay: Pin::new(Arc::new(RwLock::new(D3D11Overlay::new()))),
            imgui: Pin::new(Arc::new(RwLock::new(D3D11ImguiController::new()))),
            ipc,
        })
    }

    unsafe fn present_impl(
        handle: IpcHandle,
        mut overlay: RwLockWriteGuard<D3D11Overlay>,
        mut imgui: RwLockWriteGuard<D3D11ImguiController>,
        this: &IDXGISwapChain,
    ) -> Result<(), Box<dyn Error>> {
        // Handle update of any overlay here.
        if let Ok(cmd) = handle.try_recv() {
            match &cmd.ty {
                &GameWindowCommandType::OVERLAY => {
                    eprintln!("[dx11] received overlay texture event");
                    overlay.refresh(unsafe { cmd.params.overlay_event });
                }
                _ => {}
            }
        }

        let swapchain_desc = this.GetDesc()?;
        let backbuffer = this.GetBuffer::<ID3D11Texture2D>(0)?;

        let mut backbuffer_desc: D3D11_TEXTURE2D_DESC = Default::default();
        backbuffer.GetDesc(&mut backbuffer_desc);

        let size = backbuffer_desc.into();
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

                overlay.paint(|tid, dim| {
                    let _style_pad = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
                    let _style_border = ui.push_style_var(StyleVar::WindowBorderSize(0.0));
                    Window::new("BrowserWindow")
                        .size(dim.into(), Condition::Always)
                        .position([0.0, 0.0], Condition::Always)
                        .flags(
                            WindowFlags::NO_DECORATION
                                | WindowFlags::NO_MOVE
                                | WindowFlags::NO_RESIZE
                                | WindowFlags::NO_BACKGROUND,
                        )
                        .no_decoration()
                        .build(&ui, || {
                            Image::new(TextureId::new(tid), dim.into()).build(&ui)
                        })
                        .unwrap_or_else(|| eprintln!("[imgui] Unable to build window"));
                });
                ui.show_demo_window(&mut false);
                render.render(ui.render()).unwrap()
            });

            overlay.release_sync();
        }

        Ok::<_, Box<dyn Error>>(())
    }

    fn resize_impl(mut imgui: RwLockWriteGuard<D3D11ImguiController>) {
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
                    unsafe { Direct3D11Kernel::present_impl(handle, overlay, imgui, &this) }
                        .unwrap_or(());
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

    pub fn init(&mut self) -> Result<(), Box<dyn Error>> {
        println!("[dx11] init");
        self.hook
            .new(self.make_present(), self.make_resize())?
            .persist();

        Ok(())
    }
}
