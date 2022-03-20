use std::sync::Arc;
use imgui::{Context, DrawData};
use parking_lot::RwLock;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::OpenGL::HGLRC;
use imgui_renderer_ogl::{OpenGLImguiRenderer,  RenderToken};
use opengl_bindings::Gl;
use crate::common::{Dimensions, RenderError};
use crate::wgl::overlay::WGLOverlay;

pub (in crate::wgl) struct WGLImguiController {
    imgui: Arc<RwLock<Context>>,
    renderer: Option<OpenGLImguiRenderer>,
    window: HWND,
    ctx: HGLRC,
}

pub(in crate::wgl) struct Render<'a> {
    render: Option<&'a mut OpenGLImguiRenderer>,
}

impl Render<'_> {
    pub fn render(self, draw_data: &DrawData) -> Result<RenderToken, RenderError> {
        if let Some(renderer) = self.render {
            Ok(renderer.render(draw_data))
        } else {
            Err(RenderError::RendererNotReady)
        }
    }
}

impl WGLImguiController {
    pub fn new(imgui: Arc<RwLock<Context>>) -> WGLImguiController {
        WGLImguiController {
            imgui,
            renderer: None,
            window: HWND(0),
            ctx: HGLRC(0),
        }
    }

    pub const fn renderer_ready(&self) -> bool {
        self.renderer.is_some()
    }

    fn init_renderer(&mut self, gl: &Gl, window: HWND) -> Result<(), RenderError> {
        // Renderer owns its device.
        self.renderer = Some(OpenGLImguiRenderer::new(&gl, &mut self.imgui.write())?);
        self.window = window;
        Ok(())
    }

    pub fn invalidate_renderer(&mut self) {
        self.renderer = None;
    }

    pub fn frame<'a, F: FnOnce(&mut Context, Render, &mut WGLOverlay) -> Result<RenderToken, RenderError>>(
        &mut self,
        overlay: &mut WGLOverlay,
        f: F,
    ) -> Result<RenderToken, RenderError> {
        let renderer = Render {
            render: self.renderer.as_mut(),
        };

        f(&mut self.imgui.write(), renderer, overlay)
    }

    #[must_use]
    pub fn prepare_paint(&mut self, gl: &Gl, window: HWND, ctx: HGLRC, screen_dim: Dimensions) -> Result<(), RenderError> {
        if window != self.window || ctx != self.ctx {
            eprintln!("[wgl] render context changed");
            self.invalidate_renderer();
        }

        if !self.renderer_ready() {
            self.init_renderer(gl, window)?;
        }

        // set screen size..
        self.imgui.write().io_mut().display_size = screen_dim.into();
        self.window = window;
        self.ctx = ctx;
        Ok(())
    }
}

unsafe impl Send for WGLImguiController {}
unsafe impl Sync for WGLImguiController {}