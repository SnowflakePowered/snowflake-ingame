mod renderer;
mod device_objects;
mod backup;

use imgui::TextureId;
use opengl_bindings::types::{GLenum, GLuint};

pub trait ImguiTexture {
    fn as_tex_id(&self) -> TextureId;
}

impl ImguiTexture for GLuint {
    fn as_tex_id(&self) -> TextureId {
        TextureId::new(*self as usize)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    #[error("Failed to compile shader type {0:x} with GLSL {1}")]
    CompileError(GLenum, &'static str),

    #[error("Failed to link shader")]
    LinkError,

    #[error("Missing required extensions: {0}")]
    MissingExtensionError(&'static str)
}

pub use renderer::Renderer as OpenGLImguiRenderer;
pub use renderer::RenderToken;
