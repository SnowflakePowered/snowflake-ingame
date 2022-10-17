mod backup;
mod device_objects;
mod renderer;

use imgui::TextureId;
use opengl_bindings::types::{GLenum, GLuint};

pub trait ImguiTexture {
    fn as_tex_id(&self) -> TextureId;
}

impl ImguiTexture for GLuint {
    fn as_tex_id(&self) -> TextureId {
        static_assertions::const_assert!(
            std::mem::size_of::<GLuint>() <= std::mem::size_of::<usize>()
        );
        TextureId::new(*self as usize)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    #[error("Failed to compile shader type {0:x} with GLSL {1}")]
    CompileError(GLenum, Box<&'static str>),

    #[error("Failed to link shader")]
    LinkError,

    #[error("Missing required extensions: {0}")]
    MissingExtensionError(Box<&'static str>),
}

pub use renderer::RenderToken;
pub use renderer::Renderer as OpenGLImguiRenderer;
