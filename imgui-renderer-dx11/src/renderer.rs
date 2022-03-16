use std::ffi::OsString;
use std::marker::PhantomData;
use std::os::windows::ffi::OsStrExt;
use std::str::FromStr;
use imgui::internal::RawWrapper;
use imgui::{DrawCmd, DrawData, DrawIdx, DrawVert};
use windows::core::{HSTRING, Result as HResult};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, D3D11_MAP_WRITE_DISCARD,
    D3D11_VIEWPORT,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32_UINT};
use windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_RESET;

use crate::backup::StateBackup;
use crate::buffers::{IndexBuffer, VertexBuffer};
use crate::device_objects::{FontTexture, RendererDeviceObjects};

#[repr(C)]
pub(crate) struct VertexConstantBuffer {
    mvp: [[f32; 4]; 4],
}

pub struct RenderToken<'a>(PhantomData<&'a ()>);

#[derive(Debug)]
pub struct Renderer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    device_objects: Option<RendererDeviceObjects>,
    font: Option<FontTexture>,
    vertex_buffer: VertexBuffer,
    index_buffer: IndexBuffer,
}

impl Renderer {
    fn set_imgui_metadata(imgui: &mut imgui::Context) {
        imgui.io_mut().backend_flags |= imgui::BackendFlags::RENDERER_HAS_VTX_OFFSET;
        imgui.set_renderer_name(Some(format!(
            "imgui-renderer-dx11@{}",
            env!("CARGO_PKG_VERSION")
        )));
    }

    pub fn new(device: &ID3D11Device, imgui: &mut imgui::Context) -> HResult<Self> {
        let device = device.clone();
        let mut context = None;
        unsafe { device.GetImmediateContext(&mut context) };
        let index_buffer = IndexBuffer::new(&device)?;
        let vertex_buffer = VertexBuffer::new(&device)?;
        let context = if let Some(context) = context {
            context
        } else {
            let error = OsString::from_str("Unable to get immediate context from device.")
                .unwrap(); // infallible
            let error: Vec<u16> = error.encode_wide().chain([0u16]).collect();
            return Err(windows::core::Error::new(DXGI_ERROR_DEVICE_RESET, HSTRING::from_wide(&error)).into());
        };
        let mut renderer = Renderer {
            device,
            context,
            device_objects: None,
            font: None,
            vertex_buffer,
            index_buffer,
        };
        renderer.create_device_objects(imgui)?;
        Self::set_imgui_metadata(imgui);
        Ok(renderer)
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

    pub fn render<'a>(&mut self, draw_data: &DrawData) -> HResult<RenderToken<'a>> {
        // Avoid rendering when minimized
        if draw_data.display_size[0] <= 0.0
            || draw_data.display_size[1] <= 0.0
            || draw_data.draw_lists_count() == 0
        {
            return Ok(RenderToken(PhantomData));
        }

        self.vertex_buffer
            .reserve(draw_data.total_vtx_count as usize)?;
        self.index_buffer
            .reserve(draw_data.total_idx_count as usize)?;

        unsafe {
            let state = StateBackup::new(&self.context);
            self.upload_buffers(draw_data)?;
            self.setup_render_state(draw_data);
            self.render_cmd_lists(draw_data);
            drop(state)
        }
        Ok(RenderToken(PhantomData))
    }

    // upload vertex/index data into a single contiguous GPU buffer
    unsafe fn upload_buffers(&self, draw_data: &DrawData) -> HResult<()> {
        // Scope guard for mapped resources
        // Vertex and index buffers
        {
            // Does this unmap on failure?
            let vtx_resource =
                self.context
                    .Map(self.vertex_buffer.buffer(), 0, D3D11_MAP_WRITE_DISCARD, 0)?;
            let idx_resource =
                self.context
                    .Map(self.index_buffer.buffer(), 0, D3D11_MAP_WRITE_DISCARD, 0)?;

            let mut vtx_dst = std::slice::from_raw_parts_mut(
                vtx_resource.pData.cast::<DrawVert>(),
                draw_data.total_vtx_count as usize,
            );
            let mut idx_dst = std::slice::from_raw_parts_mut(
                idx_resource.pData.cast::<DrawIdx>(),
                draw_data.total_idx_count as usize,
            );

            // https://github.com/Veykril/imgui-dx11-renderer/blob/master/src/lib.rs#L373
            for (vbuf, ibuf) in draw_data
                .draw_lists()
                .map(|draw_list| (draw_list.vtx_buffer(), draw_list.idx_buffer()))
            {
                vtx_dst[..vbuf.len()].copy_from_slice(vbuf);
                idx_dst[..ibuf.len()].copy_from_slice(ibuf);
                vtx_dst = &mut vtx_dst[vbuf.len()..];
                idx_dst = &mut idx_dst[ibuf.len()..];
            }

            self.context.Unmap(self.vertex_buffer.buffer(), 0);
            self.context.Unmap(self.index_buffer.buffer(), 0);
        }

        // Setup orthographic projection matrix into our constant buffer
        // Our visible imgui space lies from drawData->DisplayPos (top left) to drawData->DisplayPos+dataData->DisplaySize (bottom right).
        // DisplayPos is (0,0) for single viewport apps.
        if let Some(device_objects) = &self.device_objects {
            let const_resource = self.context.Map(
                &device_objects.vertex_constant_buffer,
                0,
                D3D11_MAP_WRITE_DISCARD,
                0,
            )?;

            let l = draw_data.display_pos[0];
            let r = draw_data.display_pos[0] + draw_data.display_size[0];
            let t = draw_data.display_pos[1];
            let b = draw_data.display_pos[1] + draw_data.display_size[1];
            let mvp = [
                [2.0 / (r - l), 0.0, 0.0, 0.0],
                [0.0, 2.0 / (t - b), 0.0, 0.0],
                [0.0, 0.0, 0.5, 0.0],
                [(r + l) / (l - r), (t + b) / (b - t), 0.5, 1.0],
            ];
            *const_resource.pData.cast::<VertexConstantBuffer>() = VertexConstantBuffer { mvp };
            self.context
                .Unmap(&device_objects.vertex_constant_buffer, 0);
        }
        Ok(())
    }

    unsafe fn setup_render_state(&self, draw_data: &DrawData) {
        let ctx = &self.context;
        if let Some(device_objects) = &self.device_objects {
            let viewport = D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: draw_data.display_size[0],
                Height: draw_data.display_size[1],
                MinDepth: 0.0,
                MaxDepth: 1.0,
            };

            ctx.RSSetViewports(&[viewport]);

            let stride = std::mem::size_of::<DrawVert>() as u32;
            ctx.IASetInputLayout(&device_objects.input_layout);
            ctx.IASetVertexBuffers(
                0,
                1,
                &self.vertex_buffer.buffer().clone().into(),
                &stride,
                &0,
            );
            ctx.IASetIndexBuffer(
                self.index_buffer.buffer(),
                IDX_FORMAT,
                0,
            );
            ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.VSSetShader(&device_objects.vertex_shader, &[]);
            ctx.VSSetConstantBuffers(0, &[device_objects.vertex_constant_buffer.clone().into()]);
            ctx.PSSetShader(&device_objects.pixel_shader, &[]);

            if let Some(font) = &self.font {
                ctx.PSSetSamplers(0, &[font.font_sampler.clone().into()]);
            }
            ctx.GSSetShader(None, &[]);
            ctx.HSSetShader(None, &[]);
            ctx.DSSetShader(None, &[]);
            ctx.CSSetShader(None, &[]);

            let blend_factor = [0.0; 4];
            ctx.OMSetBlendState(
                &device_objects.blend_state,
                blend_factor.as_ptr(),
                0xFFFFFFFF,
            );
            ctx.OMSetDepthStencilState(&device_objects.depth_stencil_state, 0);
            ctx.RSSetState(&device_objects.rasterizer_state);
        }
    }

    unsafe fn render_cmd_lists(&self, draw_data: &DrawData) {
        let clip_off = draw_data.display_pos;
        let clip_scale = draw_data.framebuffer_scale;
        let mut vertex_offset = 0;
        let mut index_offset = 0;
        for draw_list in draw_data.draw_lists() {
            for cmd in draw_list.commands() {
                match cmd {
                    DrawCmd::RawCallback { callback, raw_cmd } => {
                        callback(draw_list.raw(), raw_cmd)
                    }
                    DrawCmd::ResetRenderState => self.setup_render_state(draw_data),
                    DrawCmd::Elements { count, cmd_params } => {
                        // X Y Z W
                        let [clip_x, clip_y, clip_z, clip_w] = cmd_params.clip_rect;
                        let [off_x, off_y] = clip_off;
                        let [scale_x, scale_y] = clip_scale;

                        let clip_min = (
                            clip_x - off_x,
                            clip_y - off_y,
                        );
                        let clip_max = (
                            clip_z - off_x,
                            clip_w - off_y,
                        );

                        if clip_max.0 <= clip_min.0 || clip_max.1 <= clip_min.1 {
                            continue;
                        }

                        let rect = RECT {
                            left: (clip_min.0 * scale_x) as i32,
                            top: (clip_min.1 * scale_y) as i32,
                            right: (clip_max.0 * scale_x) as i32,
                            bottom: (clip_max.1 * scale_y) as i32,
                        };

                        // Apply scissor/clipping rectangle
                        self.context.RSSetScissorRects(&[rect]);

                        // srv will be dropped after rendering.
                        let texture_srv: ID3D11ShaderResourceView =
                            std::mem::transmute(cmd_params.texture_id.id());

                        // Bind texture, Draw
                        self.context
                            .PSSetShaderResources(0, &[texture_srv.clone().into()]);
                        self.context.DrawIndexed(
                            count as u32,
                            (cmd_params.idx_offset + index_offset) as u32,
                            (cmd_params.vtx_offset + vertex_offset) as i32,
                        );
                    }
                }
            }

            vertex_offset += draw_list.vtx_buffer().len();
            index_offset += draw_list.idx_buffer().len();
        }
    }
}

const IDX_FORMAT: DXGI_FORMAT = idx_format();
const fn idx_format() -> DXGI_FORMAT {
    if std::mem::size_of::<DrawIdx>() == 2 {
        DXGI_FORMAT_R16_UINT
    } else {
        DXGI_FORMAT_R32_UINT
    }
}