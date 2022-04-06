#![feature(strict_provenance)]

mod backup;
mod buffers;
mod device_objects;
mod renderer;

use imgui::TextureId;
use windows::Win32::Graphics::Direct3D11::ID3D11ShaderResourceView;

#[derive(thiserror::Error, Debug)]
pub enum RenderError {
    #[error("DirectX Error: {0:?}")]
    DirectXError(#[from] windows::core::Error),

    #[error("Unable to get immediate context from device.")]
    ContextInitError,
}

pub trait ImguiTexture {
    fn as_tex_id(&self) -> TextureId;
}

#[cfg(not(feature = "strict-provenance"))]
impl ImguiTexture for ID3D11ShaderResourceView {
    fn as_tex_id(&self) -> TextureId {
        static_assertions::assert_eq_size!(ID3D11ShaderResourceView, usize);
        let srv = self.clone();
        unsafe {
            TextureId::from(std::mem::transmute::<_, usize>(srv))
        }
    }
}

#[cfg(feature = "strict-provenance")]
impl ImguiTexture for ID3D11ShaderResourceView {
    fn as_tex_id(&self) -> TextureId {
        static_assertions::assert_eq_size!(ID3D11ShaderResourceView,
            core::ptr::NonNull<core::ffi::c_void>);

        let srv = self.clone();

        // SAFETY: repr(transparent)
        let ptr =  unsafe { std::mem::transmute::<_, core::ptr::NonNull<core::ffi::c_void>>(srv) };
        TextureId::from(ptr.as_ptr().expose_addr())
    }
}


pub use renderer::Renderer as Direct3D11ImguiRenderer;
pub use renderer::RenderToken;