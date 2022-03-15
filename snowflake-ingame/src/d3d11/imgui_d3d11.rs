use std::mem::MaybeUninit;
use std::ptr;
use imgui::{Context, DrawData};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11RenderTargetView, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_SWAP_CHAIN_DESC, IDXGISwapChain};
use imgui_renderer_dx11::Direct3D11ImguiRenderer;
use crate::common::Dimensions;
use crate::d3d11::overlay_d3d11::Direct3D11Overlay;
use windows::core::Result as HResult;

pub(in crate::d3d11) struct Render<'a> {
    render: Option<&'a mut Direct3D11ImguiRenderer>,
}

impl Render<'_> {
    pub fn render(self, draw_data: &DrawData) -> HResult<()> {
        if let Some(renderer) = self.render {
            renderer.render(draw_data)?;
        }
        Ok(())
    }
}


pub(in crate::d3d11) struct Direct3D11ImguiController {
    imgui: Context,
    renderer: Option<Direct3D11ImguiRenderer>,
    device: Option<ID3D11Device>,
    window: HWND,
    rtv: Option<ID3D11RenderTargetView>,
}

impl Direct3D11ImguiController {
    pub fn new() -> Direct3D11ImguiController {
        Direct3D11ImguiController {
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
    pub fn frame<'a, F: FnOnce(&mut Context, Render, &mut Direct3D11Overlay) -> ()>(
        &mut self,
        overlay: &mut Direct3D11Overlay,
        f: F,
    ) {
        let renderer = Render {
            render: self.renderer.as_mut(),
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

unsafe impl Send for Direct3D11ImguiController {}
unsafe impl Sync for Direct3D11ImguiController {}