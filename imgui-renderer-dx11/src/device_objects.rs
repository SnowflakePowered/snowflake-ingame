use windows::core::{PCSTR, Result as HResult};
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_SHADER_RESOURCE, D3D11_BLEND_DESC, D3D11_BLEND_INV_SRC_ALPHA, D3D11_BLEND_OP_ADD, D3D11_BLEND_SRC_ALPHA, D3D11_BLEND_ZERO, D3D11_BUFFER_DESC, D3D11_COLOR_WRITE_ENABLE_ALL, D3D11_COMPARISON_ALWAYS, D3D11_CPU_ACCESS_WRITE, D3D11_CULL_NONE, D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_STENCILOP_DESC, D3D11_DEPTH_WRITE_MASK_ALL, D3D11_FILL_SOLID, D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC, D3D11_RESOURCE_MISC_FLAG, D3D11_SAMPLER_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0, D3D11_STENCIL_OP_KEEP, D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_SRV, D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_WRAP, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC, ID3D11BlendState, ID3D11Buffer, ID3D11DepthStencilState, ID3D11Device, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState, ID3D11SamplerState, ID3D11ShaderResourceView, ID3D11VertexShader};
use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
};

use crate::ImguiTexture;
use crate::renderer::VertexConstantBuffer;

const VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/vertex_shader.vs_4_0"));
const PIXEL_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pixel_shader.ps_4_0"));

#[derive(Debug)]
pub(crate) struct RendererDeviceObjects {
    pub vertex_shader: ID3D11VertexShader,
    pub vertex_constant_buffer: ID3D11Buffer,
    pub input_layout: ID3D11InputLayout,
    pub pixel_shader: ID3D11PixelShader,
    pub blend_state: ID3D11BlendState,
    pub rasterizer_state: ID3D11RasterizerState,
    pub depth_stencil_state: ID3D11DepthStencilState,
}

#[derive(Debug)]
pub(crate) struct FontTexture {
    pub font_resource_view: ID3D11ShaderResourceView,
    pub font_sampler: ID3D11SamplerState,
}

impl FontTexture {
    pub fn tex_id(&self) -> imgui::TextureId {
        self.font_resource_view.as_tex_id()
    }

    pub fn new(
        fonts: &mut imgui::FontAtlasRefMut<'_>,
        device: &ID3D11Device,
    ) -> HResult<FontTexture> {
        let font_tex_data = fonts.build_rgba32_texture();
        let tex_desc = D3D11_TEXTURE2D_DESC {
            Width: font_tex_data.width,
            Height: font_tex_data.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE,
            ..Default::default()
        };

        let tex_sub_rsrc = D3D11_SUBRESOURCE_DATA {
            pSysMem: font_tex_data.data.as_ptr().cast(),
            SysMemPitch: tex_desc.Width * 4,
            SysMemSlicePitch: 0,
        };

        let font_tex = unsafe { device.CreateTexture2D(&tex_desc, Some(&tex_sub_rsrc))? };

        let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 1,
                },
            },
        };

        let font_srv = unsafe { device.CreateShaderResourceView(&font_tex, Some(&srv_desc))? };

        let font_sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_WRAP,
            AddressV: D3D11_TEXTURE_ADDRESS_WRAP,
            AddressW: D3D11_TEXTURE_ADDRESS_WRAP,
            MipLODBias: 0.0,
            MaxAnisotropy: 0,
            ComparisonFunc: D3D11_COMPARISON_ALWAYS,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: 0.0,
        };

        let font_sampler = unsafe { device.CreateSamplerState(&font_sampler_desc)? };

        Ok(FontTexture {
            font_resource_view: font_srv,
            font_sampler,
        })
    }
}

impl RendererDeviceObjects {
    pub fn new(device: &ID3D11Device) -> HResult<RendererDeviceObjects> {
        let (vertex_shader, input_layout) = create_vertex_shader(device)?;
        let vertex_constant_buffer = create_vertex_const_buffer(device)?;
        let pixel_shader = create_pixel_shader(device)?;
        let blend_state = create_blend_state(device)?;
        let rasterizer_state = create_rasterizer_state(device)?;
        let depth_stencil_state = create_stencil_state(device)?;
        Ok(RendererDeviceObjects {
            vertex_shader,
            vertex_constant_buffer,
            input_layout,
            pixel_shader,
            blend_state,
            rasterizer_state,
            depth_stencil_state,
        })
    }
}

#[must_use]
fn create_pixel_shader(device: &ID3D11Device) -> HResult<ID3D11PixelShader> {
    unsafe { device.CreatePixelShader(&PIXEL_SHADER, None) }
}

#[must_use]
fn create_vertex_const_buffer(device: &ID3D11Device) -> HResult<ID3D11Buffer> {
    let buffer_desc = D3D11_BUFFER_DESC {
        ByteWidth: std::mem::size_of::<VertexConstantBuffer>() as u32,
        Usage: D3D11_USAGE_DYNAMIC,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER,
        CPUAccessFlags: D3D11_CPU_ACCESS_WRITE,
        MiscFlags: D3D11_RESOURCE_MISC_FLAG(0),
        StructureByteStride: 0,
    };
    unsafe { device.CreateBuffer(&buffer_desc, None) }
}

#[must_use]
fn create_vertex_shader(device: &ID3D11Device) -> HResult<(ID3D11VertexShader, ID3D11InputLayout)> {
    let local_layout = [
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(b"POSITION\0".as_ptr()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 0,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(b"TEXCOORD\0".as_ptr()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlot: 0,
            AlignedByteOffset: 8,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(b"COLOR\0".as_ptr()),
            SemanticIndex: 0,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            InputSlot: 0,
            AlignedByteOffset: 16,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            InstanceDataStepRate: 0,
        },
    ];

    let vertex_shader = unsafe { device.CreateVertexShader(&VERTEX_SHADER, None)? };
    let input_layout = unsafe { device.CreateInputLayout(&local_layout, &VERTEX_SHADER)? };

    Ok((vertex_shader, input_layout))
}

#[must_use]
fn create_blend_state(device: &ID3D11Device) -> HResult<ID3D11BlendState> {
    let mut blend_desc = D3D11_BLEND_DESC {
        AlphaToCoverageEnable: false.into(),
        IndependentBlendEnable: false.into(),
        RenderTarget: [Default::default(); 8],
    };

    blend_desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        SrcBlend: D3D11_BLEND_SRC_ALPHA,
        DestBlend: D3D11_BLEND_INV_SRC_ALPHA,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_INV_SRC_ALPHA,
        DestBlendAlpha: D3D11_BLEND_ZERO,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };

    unsafe { device.CreateBlendState(&blend_desc) }
}

#[must_use]
fn create_rasterizer_state(device: &ID3D11Device) -> HResult<ID3D11RasterizerState> {
    let rasterizer_desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: D3D11_CULL_NONE,
        FrontCounterClockwise: false.into(),
        DepthBias: 0,
        DepthBiasClamp: 0.0,
        SlopeScaledDepthBias: 0.0,
        DepthClipEnable: true.into(),
        ScissorEnable: true.into(),
        MultisampleEnable: false.into(),
        AntialiasedLineEnable: false.into(),
    };

    unsafe { device.CreateRasterizerState(&rasterizer_desc) }
}

#[must_use]
fn create_stencil_state(device: &ID3D11Device) -> HResult<ID3D11DepthStencilState> {
    let stencil_state = D3D11_DEPTH_STENCIL_DESC {
        DepthEnable: false.into(),
        DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ALL,
        DepthFunc: D3D11_COMPARISON_ALWAYS,
        StencilEnable: false.into(),
        StencilReadMask: 0,
        StencilWriteMask: 0,
        FrontFace: D3D11_DEPTH_STENCILOP_DESC {
            StencilFailOp: D3D11_STENCIL_OP_KEEP,
            StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
            StencilPassOp: D3D11_STENCIL_OP_KEEP,
            StencilFunc: D3D11_COMPARISON_ALWAYS,
        },
        BackFace: D3D11_DEPTH_STENCILOP_DESC {
            StencilFailOp: D3D11_STENCIL_OP_KEEP,
            StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
            StencilPassOp: D3D11_STENCIL_OP_KEEP,
            StencilFunc: D3D11_COMPARISON_ALWAYS,
        },
    };

    unsafe { device.CreateDepthStencilState(&stencil_state) }
}
