use imgui::TextureId;
use opengl_bindings::types::GLuint;

mod renderer;
mod device_objects;
mod backup;

pub trait ImguiTexture {
    fn as_tex_id(&self) -> TextureId;
}

impl ImguiTexture for GLuint {
    fn as_tex_id(&self) -> TextureId {
        TextureId::new(*self as usize)
    }
}

pub use renderer::Renderer as OpenGLImguiRenderer;
pub use renderer::RenderToken;