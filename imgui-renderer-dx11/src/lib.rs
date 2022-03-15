mod backup;
mod buffers;
mod device_objects;
mod renderer;

use imgui::TextureId;
use windows::Win32::Graphics::Direct3D11::ID3D11ShaderResourceView;
pub use renderer::Renderer as Direct3D11ImguiRenderer;
pub use renderer::RenderToken;

pub trait ImguiTexture {
    fn as_tex_id(&self) -> TextureId;
}

impl ImguiTexture for ID3D11ShaderResourceView {
    fn as_tex_id(&self) -> TextureId {
        let srv = self.clone();
        unsafe {
            TextureId::from(std::mem::transmute::<_, *const ()>(srv) as usize)
        }
    }
}