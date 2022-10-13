use std::ptr;
use std::sync::Arc;
use imgui::{Context, DrawData};
use parking_lot::RwLock;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11RenderTargetView, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::{DXGI_SWAP_CHAIN_DESC, IDXGISwapChain};
use imgui_renderer_dx11::{Direct3D11ImguiRenderer, RenderToken};
use crate::common::{Dimensions, RenderError};
use crate::d3d11::overlay_d3d11::Direct3D11Overlay;
use windows::core::Result as HResult;

pub(in crate::d3d11) struct Render<'a> {
    render: Option<&'a mut Direct3D11ImguiRenderer>,
}

impl Render<'_> {
    pub fn render(self, draw_data: &DrawData) -> Result<RenderToken, RenderError> {
        if let Some(renderer) = self.render {
            Ok(renderer.render(draw_data)?)
        } else {
            Err(RenderError::RendererNotReady)
        }
    }
}

pub(in crate::d3d11) struct Direct3D11ImguiController {
    imgui: Arc<RwLock<imgui::Context>>,
    renderer: Option<Direct3D11ImguiRenderer>,
    window: HWND,
    rtv: Option<ID3D11RenderTargetView>,
}

impl Direct3D11ImguiController {
    pub fn new(imgui: Arc<RwLock<imgui::Context>>) -> Direct3D11ImguiController {
        Direct3D11ImguiController {
            imgui,
            renderer: None,
            window: HWND(0),
            rtv: None,
        }
    }

    #[inline]
    const fn renderer_ready(&self) -> bool {
        self.renderer.is_some()
    }

    #[inline]
    const fn rtv_ready(&self) -> bool {
        self.rtv.is_some()
    }

    pub fn frame<'a, F: FnOnce(&mut Context, Render, &mut Direct3D11Overlay) -> Result<RenderToken, RenderError>>(
        &mut self,
        overlay: &mut Direct3D11Overlay,
        f: F,
    ) -> Result<RenderToken, RenderError> {
        let renderer = Render {
            render: self.renderer.as_mut(),
        };

        f(&mut self.imgui.write(), renderer, overlay)
    }

    fn init_renderer(&mut self, swapchain: &IDXGISwapChain, window: HWND) -> Result<(), RenderError>{
        let device = unsafe { swapchain.GetDevice()? };
        // Renderer owns its device.
        self.renderer = Some(Direct3D11ImguiRenderer::new(&device, &mut self.imgui.write())?);
        self.window = window;
        Ok(())
    }

    pub fn invalidate_renderer(&mut self) {
        self.renderer = None;
    }

    pub fn invalidate_rtv(&mut self) {
        self.rtv = None;
    }

    fn init_rtv(&mut self, swapchain: &IDXGISwapChain) -> HResult<()> {
        unsafe {
            let device: ID3D11Device = swapchain.GetDevice()?;
            let context = {
                let mut context = None;
                device.GetImmediateContext(&mut context);
                context
            };

            let mut rtv = [None];
            if let Some(context) = &context {
                context.OMGetRenderTargets(Some(&mut rtv), None);
            }

            if let Some(Some(rtv)) = rtv.into_iter().next() {
                self.rtv = Some(rtv)
            } else {
                let back_buffer: ID3D11Texture2D = swapchain.GetBuffer(0)?;
                let rtv = device.CreateRenderTargetView(back_buffer, None)?;
                if let Some(context) = &context {
                    context.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
                }
                self.rtv = Some(rtv);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn prepare_paint(&mut self, swapchain: &IDXGISwapChain, screen_dim: Dimensions) -> Result<(), RenderError> {
        let swap_desc: DXGI_SWAP_CHAIN_DESC = unsafe { swapchain.GetDesc()? };

        if swap_desc.OutputWindow != self.window {
            eprintln!("[dx11] render context changed");
            self.invalidate_renderer();
            self.invalidate_rtv();
        }

        if !self.renderer_ready() {
            self.init_renderer(swapchain, swap_desc.OutputWindow)?;
        }

        if !self.rtv_ready() {
            self.init_rtv(&swapchain)?;
        }

        // set screen size..
        self.imgui.write().io_mut().display_size = screen_dim.into();
        self.window = swap_desc.OutputWindow;
        Ok(())
    }
}

unsafe impl Send for Direct3D11ImguiController {}
unsafe impl Sync for Direct3D11ImguiController {}