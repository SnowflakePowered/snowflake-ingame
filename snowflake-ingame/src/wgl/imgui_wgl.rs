use imgui::{Context, DrawData};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::OpenGL::HGLRC;
use imgui_renderer_ogl::{OpenGLImguiRenderer, RenderError};
use opengl_bindings::Gl;
use crate::common::Dimensions;
use crate::wgl::overlay::WGLOverlay;

pub (in crate::wgl) struct WGLImguiController {
    imgui: Context,
    renderer: Option<OpenGLImguiRenderer>,
    window: HWND,
    ctx: HGLRC,
}

pub(in crate::wgl) struct Render<'a> {
    render: Option<&'a mut OpenGLImguiRenderer>,
}

impl Render<'_> {
    pub fn render(self, draw_data: &DrawData) -> Result<(), RenderError> {
        if let Some(renderer) = self.render {
            renderer.render(draw_data);
        }
        Ok(())
    }
}

impl WGLImguiController {
    pub fn new() -> WGLImguiController {
        WGLImguiController {
            imgui: Context::create(),
            renderer: None,
            window: HWND(0),
            ctx: HGLRC(0),
        }
    }

    pub const fn renderer_ready(&self) -> bool {
        self.renderer.is_some()
    }

    unsafe fn init_renderer(&mut self, gl: &Gl, window: HWND) -> Result<(), RenderError>{
        // Renderer owns its device.
        self.renderer = Some(OpenGLImguiRenderer::new(&gl, &mut self.imgui)?);
        self.window = window;
        Ok(())
    }

    pub fn invalidate_renderer(&mut self) {
        self.renderer = None;
    }

    // todo: use RenderToken POW
    pub fn frame<'a, F: FnOnce(&mut Context, Render, &mut WGLOverlay) -> ()>(
        &mut self,
        overlay: &mut WGLOverlay,
        f: F,
    ) {
        let renderer = Render {
            render: self.renderer.as_mut(),
        };

        f(&mut self.imgui, renderer, overlay);
    }

    pub fn prepare_paint(&mut self, gl: &Gl, window: HWND, ctx: HGLRC, screen_dim: Dimensions) -> bool {
        if window != self.window || ctx != self.ctx {
            eprintln!("[wgl] render context changed");
            self.invalidate_renderer();
        }

        if !self.renderer_ready() {
            if let Err(e) = unsafe { self.init_renderer(gl, window) } {
                eprintln!("[wgl] unable to initialize renderer {:?}", e);
                return false;
            }
        }

        // set screen size..
        self.imgui.io_mut().display_size = screen_dim.into();
        self.window = window;
        self.ctx = ctx;
        true
    }
}

unsafe impl Send for WGLImguiController {}
unsafe impl Sync for WGLImguiController {}