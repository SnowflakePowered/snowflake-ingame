use imgui::{BackendFlags, Font, Textures};
use windows::core::Result as HResult;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView};
use crate::device_objects::{FontTexture, RendererDeviceObjects};


const VERTEX_BUF_ADD_CAPACITY: usize = 5000;
const INDEX_BUF_ADD_CAPACITY: usize = 10000;


#[repr(C)]
pub(crate) struct VertexConstantBuffer {
    mvp: [[f32; 4]; 4],
}

#[derive(Debug)]
pub struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    textures: Textures<ID3D11ShaderResourceView>,
    device_objects: Option<RendererDeviceObjects>,
    font: Option<FontTexture>
}

impl Renderer {
    pub fn new(device: &ID3D11Device, imgui: &mut imgui::Context) -> HResult<Self> {
        let device = device.clone();
        let mut context = None;
        unsafe { device.GetImmediateContext(&mut context) };
        let mut renderer = Renderer {
            device,
            context: context.unwrap(), // todo: check unwrapped?
            textures: Textures::new(),
            device_objects: None,
            font: None
        };
        renderer.create_device_objects(imgui)?;
        Self::set_imgui_metadata(imgui);
        Ok(renderer)
    }

    fn set_imgui_metadata(imgui: &mut imgui::Context) {
        imgui.io_mut().backend_flags |= imgui::BackendFlags::RENDERER_HAS_VTX_OFFSET;
        imgui.set_renderer_name(Some(format!("imgui-renderer-dx11@{}", env!("CARGO_PKG_VERSION"))));
    }

    pub fn create_device_objects(&mut self, imgui: &mut imgui::Context) -> HResult<()> {
        let device_objects = RendererDeviceObjects::new(&self.device)?;
        let mut imgui_fonts = imgui.fonts();
        let fonts = FontTexture::new(&mut imgui_fonts, &self.device)?;
        imgui_fonts.tex_id = fonts.tex_id();
        self.font = Some(fonts);
        self.device_objects = Some(device_objects);
        Ok(())
    }


}