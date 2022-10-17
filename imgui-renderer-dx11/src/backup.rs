use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11BlendState, ID3D11Buffer, ID3D11ClassInstance, ID3D11ComputeShader,
    ID3D11DepthStencilState, ID3D11DepthStencilView, ID3D11DeviceContext, ID3D11DomainShader,
    ID3D11GeometryShader, ID3D11HullShader, ID3D11InputLayout, ID3D11PixelShader,
    ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView,
    ID3D11UnorderedAccessView, ID3D11VertexShader, D3D11_1_UAV_SLOT_COUNT,
    D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT,
    D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT, D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT,
    D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT, D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT,
    D3D11_VIEWPORT, D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;

pub struct StateBackup<'ctx> {
    _context: &'ctx ID3D11DeviceContext,
    _ia: IABackup<'ctx>,
    _rs: RSBackup<'ctx>,
    _om: OMBackup<'ctx>,
    _vs: VSBackup<'ctx>,
    _hs: HSBackup<'ctx>,
    _ds: DSBackup<'ctx>,
    _gs: GSBackup<'ctx>,
    _ps: PSBackup<'ctx>,
    _cs: CSBackup<'ctx>,
}

#[derive(Default)]
struct IABackup<'ctx> {
    _parent: Option<&'ctx ID3D11DeviceContext>,
    input_layout: Option<ID3D11InputLayout>,
    index_buffer: (Option<ID3D11Buffer>, DXGI_FORMAT, u32),
    primitive_topology: D3D_PRIMITIVE_TOPOLOGY,
    vertex_buffers: [Option<ID3D11Buffer>; D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT as usize],
    vertex_buffer_strides: [u32; D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT as usize],
    vertex_buffer_offs: [u32; D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT as usize],
}

#[derive(Default)]
struct RSBackup<'ctx> {
    _parent: Option<&'ctx ID3D11DeviceContext>,
    rs_state: Option<ID3D11RasterizerState>,
    scissor_rects: [RECT; D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as usize],
    num_scissor_rects: u32,
    viewports: [D3D11_VIEWPORT; D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE as usize],
    num_viewports: u32,
}

struct OMBackup<'ctx> {
    _parent: Option<&'ctx ID3D11DeviceContext>,
    render_target_views:
        [Option<ID3D11RenderTargetView>; D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT as usize],
    unordered_access_views: [Option<ID3D11UnorderedAccessView>; D3D11_1_UAV_SLOT_COUNT as usize],
    blend_state: (Option<ID3D11BlendState>, f32, u32),
    depth_stencil: (Option<ID3D11DepthStencilState>, u32),
    depth_stencil_view: Option<ID3D11DepthStencilView>,
}

impl Default for OMBackup<'_> {
    fn default() -> Self {
        const INIT_RTV: Option<ID3D11RenderTargetView> = None;
        const INIT_UAV: Option<ID3D11UnorderedAccessView> = None;
        Self {
            _parent: None,
            render_target_views: [INIT_RTV; D3D11_SIMULTANEOUS_RENDER_TARGET_COUNT as usize],
            unordered_access_views: [INIT_UAV; D3D11_1_UAV_SLOT_COUNT as usize],
            blend_state: (None, 0f32, 0),
            depth_stencil: (None, 0),
            depth_stencil_view: None,
        }
    }
}

impl StateBackup<'_> {
    pub unsafe fn new(context: &ID3D11DeviceContext) -> StateBackup {
        StateBackup {
            _context: context,
            _ia: IABackup::backup(&context),
            _rs: RSBackup::backup(&context),
            _om: OMBackup::backup(&context),
            _vs: VSBackup::backup(&context),
            _hs: HSBackup::backup(&context),
            _ds: DSBackup::backup(&context),
            _gs: GSBackup::backup(&context),
            _ps: PSBackup::backup(&context),
            _cs: CSBackup::backup(&context),
        }
    }
}

impl<'ctx> IABackup<'ctx> {
    unsafe fn backup(context: &'ctx ID3D11DeviceContext) -> Self {
        let mut backup: Self = Default::default();
        backup._parent = Some(context);

        context.IAGetInputLayout(Some(&mut backup.input_layout));
        context.IAGetIndexBuffer(
            Some(&mut backup.index_buffer.0),
            Some(&mut backup.index_buffer.1),
            Some(&mut backup.index_buffer.2),
        );
        context.IAGetPrimitiveTopology(&mut backup.primitive_topology);

        // Blocking on https://github.com/microsoft/windows-rs/issues/1567 to do this without as_mut_ptr
        context.IAGetVertexBuffers(
            0,
            D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT,
            Some(backup.vertex_buffers.as_mut_ptr()),
            Some(backup.vertex_buffer_strides.as_mut_ptr()),
            Some(backup.vertex_buffer_offs.as_mut_ptr()),
        );

        backup
    }
}

impl<'ctx> Drop for IABackup<'ctx> {
    fn drop(&mut self) {
        if let Some(context) = self._parent {
            unsafe {
                context.IASetInputLayout(self.input_layout.as_ref());
                context.IASetIndexBuffer(
                    self.index_buffer.0.as_ref(),
                    self.index_buffer.1,
                    self.index_buffer.2,
                );
                context.IASetPrimitiveTopology(self.primitive_topology);
                context.IASetVertexBuffers(
                    0,
                    D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT,
                    Some(self.vertex_buffers.as_ptr()),
                    Some(self.vertex_buffer_strides.as_ptr()),
                    Some(self.vertex_buffer_offs.as_ptr()),
                )
            }
        }
    }
}

impl<'ctx> RSBackup<'ctx> {
    unsafe fn backup(context: &'ctx ID3D11DeviceContext) -> Self {
        let mut backup: Self = Default::default();
        backup._parent = Some(context);

        context.RSGetState(Some(&mut backup.rs_state));

        // input expects Option but really its not an option.
        // the transmute here is sus but what can we do about it?
        backup.num_scissor_rects = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
        context.RSGetScissorRects(
            &mut backup.num_scissor_rects,
            Some(backup.scissor_rects.as_mut_ptr()),
        );

        backup.num_viewports = D3D11_VIEWPORT_AND_SCISSORRECT_OBJECT_COUNT_PER_PIPELINE;
        context.RSGetViewports(
            &mut backup.num_viewports,
            Some(backup.viewports.as_mut_ptr()),
        );
        backup
    }
}

impl<'ctx> Drop for RSBackup<'ctx> {
    fn drop(&mut self) {
        if let Some(context) = self._parent {
            unsafe {
                context.RSSetState(self.rs_state.as_ref());
                context.RSSetScissorRects(Some(
                    &self.scissor_rects[..self.num_scissor_rects as usize],
                ));
                context.RSSetViewports(Some(&self.viewports[..self.num_viewports as usize]));
            }
        }
    }
}

impl<'ctx> OMBackup<'ctx> {
    unsafe fn backup(context: &'ctx ID3D11DeviceContext) -> Self {
        let mut backup: Self = Default::default();
        backup._parent = Some(context);
        context.OMGetBlendState(
            Some(&mut backup.blend_state.0),
            Some(&mut backup.blend_state.1),
            Some(&mut backup.blend_state.2),
        );
        context.OMGetDepthStencilState(
            Some(&mut backup.depth_stencil.0),
            Some(&mut backup.depth_stencil.1),
        );
        context.OMGetRenderTargetsAndUnorderedAccessViews(
            Some(&mut backup.render_target_views),
            Some(&mut backup.depth_stencil_view),
            0,
            Some(&mut backup.unordered_access_views),
        );

        backup
    }
}

impl<'ctx> Drop for OMBackup<'ctx> {
    fn drop(&mut self) {
        if let Some(context) = self._parent {
            unsafe {
                context.OMSetBlendState(
                    self.blend_state.0.as_ref(),
                    Some(&self.blend_state.1),
                    self.blend_state.2,
                );
                context.OMSetDepthStencilState(self.depth_stencil.0.as_ref(), self.depth_stencil.1);

                // yeah.. this is tough.
                let uav_initial = [u32::MAX; D3D11_1_UAV_SLOT_COUNT as usize];
                context.OMSetRenderTargetsAndUnorderedAccessViews(
                    Some(&self.render_target_views),
                    self.depth_stencil_view.as_ref(),
                    0,
                    D3D11_1_UAV_SLOT_COUNT,
                    Some(self.unordered_access_views.as_ptr()),
                    Some(uav_initial.as_ptr()),
                );
            }
        }
    }
}

const D3D11_MAX_CLASS_INSTANCES: usize = 256;

macro_rules! make_shader_backup {
    ($backup:ident => $com:ty, get { $get_shader:ident; $get_samplers:ident; $get_shader_resources:ident; $get_const_buffers:ident; }, set { $set_shader:ident; $set_samplers:ident; $set_shader_resources:ident; $set_const_buffers: ident; }
    ) => {
        struct $backup<'ctx> {
            _parent: Option<&'ctx ID3D11DeviceContext>,
            shader_state: Option<$com>,
            num_instances: u32,
            class_instances: [Option<ID3D11ClassInstance>; D3D11_MAX_CLASS_INSTANCES], // 256 is max according to PSSetShader documentation
            sampler_states:
                [Option<ID3D11SamplerState>; D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT as usize],
            resource_views: [Option<ID3D11ShaderResourceView>;
                D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
            const_buffers:
                [Option<ID3D11Buffer>; D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT as usize],
        }

        impl Default for $backup<'_> {
            fn default() -> Self {
                const INIT_CLS: Option<ID3D11ClassInstance> = None;
                const INIT_SAMPLER: Option<ID3D11SamplerState> = None;
                const INIT_SRV: Option<ID3D11ShaderResourceView> = None;
                const INIT_BUF: Option<ID3D11Buffer> = None;

                Self {
                    _parent: None,
                    shader_state: None,
                    num_instances: 0,
                    class_instances: [INIT_CLS; D3D11_MAX_CLASS_INSTANCES],
                    sampler_states: [INIT_SAMPLER; D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT as usize],
                    resource_views: [INIT_SRV;
                        D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
                    const_buffers: [INIT_BUF;
                        D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT as usize],
                }
            }
        }

        impl<'ctx> $backup<'ctx> {
            unsafe fn backup(context: &'ctx ID3D11DeviceContext) -> Self {
                let mut backup: Self = Default::default();
                backup.num_instances = D3D11_MAX_CLASS_INSTANCES as u32;
                context.$get_shader(
                    Some(&mut backup.shader_state),
                    Some(backup.class_instances.as_mut_ptr()),
                    Some(&mut backup.num_instances),
                );
                context.$get_samplers(0, Some(&mut backup.sampler_states));
                context.$get_shader_resources(0, Some(&mut backup.resource_views));
                context.$get_const_buffers(0, Some(&mut backup.const_buffers));
                backup
            }
        }

        impl<'ctx> Drop for $backup<'ctx> {
            fn drop(&mut self) {
                if let Some(context) = self._parent {
                    unsafe {
                        context
                            .$set_shader(self.shader_state.as_ref(), Some(&self.class_instances));
                        context.$set_samplers(0, Some(&self.sampler_states));
                        context.$set_shader_resources(0, Some(&self.resource_views));
                        context.$set_const_buffers(0, Some(&self.const_buffers));
                    }
                }
            }
        }
    };
}

make_shader_backup!(VSBackup => ID3D11VertexShader, get {
    VSGetShader;
    VSGetSamplers;
    VSGetShaderResources;
    VSGetConstantBuffers;
}, set {
    VSSetShader;
    VSSetSamplers;
    VSSetShaderResources;
    VSSetConstantBuffers;
});

make_shader_backup!(HSBackup => ID3D11HullShader, get {
    HSGetShader;
    HSGetSamplers;
    HSGetShaderResources;
    HSGetConstantBuffers;
}, set {
    HSSetShader;
    HSSetSamplers;
    HSSetShaderResources;
    HSSetConstantBuffers;
});

make_shader_backup!(DSBackup => ID3D11DomainShader, get {
    DSGetShader;
    DSGetSamplers;
    DSGetShaderResources;
    DSGetConstantBuffers;
}, set {
    DSSetShader;
    DSSetSamplers;
    DSSetShaderResources;
    DSSetConstantBuffers;
});

make_shader_backup!(GSBackup => ID3D11GeometryShader, get {
    GSGetShader;
    GSGetSamplers;
    GSGetShaderResources;
    GSGetConstantBuffers;
}, set {
    GSSetShader;
    GSSetSamplers;
    GSSetShaderResources;
    GSSetConstantBuffers;
});

make_shader_backup!(PSBackup => ID3D11PixelShader, get {
    PSGetShader;
    PSGetSamplers;
    PSGetShaderResources;
    PSGetConstantBuffers;
}, set {
    PSSetShader;
    PSSetSamplers;
    PSSetShaderResources;
    PSSetConstantBuffers;
});

struct CSBackup<'ctx> {
    _parent: Option<&'ctx ID3D11DeviceContext>,
    shader_state: Option<ID3D11ComputeShader>,
    num_instances: u32,
    class_instances: [Option<ID3D11ClassInstance>; D3D11_MAX_CLASS_INSTANCES],
    // 256 is max according to PSSetShader documentation
    sampler_states: [Option<ID3D11SamplerState>; D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT as usize],
    resource_views:
        [Option<ID3D11ShaderResourceView>; D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
    const_buffers:
        [Option<ID3D11Buffer>; D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT as usize],
    unordered_access_views: [Option<ID3D11UnorderedAccessView>; D3D11_1_UAV_SLOT_COUNT as usize],
}

impl Default for CSBackup<'_> {
    fn default() -> Self {
        const INIT_CLS: Option<ID3D11ClassInstance> = None;
        const INIT_SAMPLER: Option<ID3D11SamplerState> = None;
        const INIT_SRV: Option<ID3D11ShaderResourceView> = None;
        const INIT_BUF: Option<ID3D11Buffer> = None;
        const INIT_UAV: Option<ID3D11UnorderedAccessView> = None;

        Self {
            _parent: None,
            shader_state: None,
            num_instances: 0,
            class_instances: [INIT_CLS; D3D11_MAX_CLASS_INSTANCES],
            sampler_states: [INIT_SAMPLER; D3D11_COMMONSHADER_SAMPLER_SLOT_COUNT as usize],
            resource_views: [INIT_SRV; D3D11_COMMONSHADER_INPUT_RESOURCE_SLOT_COUNT as usize],
            const_buffers: [INIT_BUF; D3D11_COMMONSHADER_CONSTANT_BUFFER_API_SLOT_COUNT as usize],
            unordered_access_views: [INIT_UAV; D3D11_1_UAV_SLOT_COUNT as usize],
        }
    }
}

impl<'ctx> CSBackup<'ctx> {
    unsafe fn backup(context: &'ctx ID3D11DeviceContext) -> Self {
        let mut backup: Self = Default::default();
        backup.num_instances = D3D11_MAX_CLASS_INSTANCES as u32;
        context.CSGetShader(
            Some(&mut backup.shader_state),
            Some(backup.class_instances.as_mut_ptr()),
            Some(&mut backup.num_instances),
        );
        context.CSGetSamplers(0, Some(&mut backup.sampler_states));
        context.CSGetShaderResources(0, Some(&mut backup.resource_views));
        context.CSGetConstantBuffers(0, Some(&mut backup.const_buffers));
        context.CSGetUnorderedAccessViews(0, Some(&mut backup.unordered_access_views));
        backup
    }
}

impl<'ctx> Drop for CSBackup<'ctx> {
    fn drop(&mut self) {
        if let Some(context) = self._parent {
            unsafe {
                context.CSSetShader(self.shader_state.as_ref(), Some(&self.class_instances));
                context.CSSetSamplers(0, Some(&self.sampler_states));
                context.CSSetShaderResources(0, Some(&self.resource_views));
                context.CSSetConstantBuffers(0, Some(&self.const_buffers));
                let uav_initial = [u32::MAX; D3D11_1_UAV_SLOT_COUNT as usize];

                context.CSSetUnorderedAccessViews(
                    0,
                    D3D11_1_UAV_SLOT_COUNT,
                    Some(self.unordered_access_views.as_ptr()),
                    Some(uav_initial.as_ptr()),
                );
            }
        }
    }
}
