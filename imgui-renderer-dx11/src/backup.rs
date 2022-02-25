use std::marker::PhantomData;
use windows::Win32::Graphics::Direct3D11::{D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT, ID3D11Buffer, ID3D11DeviceContext, ID3D11InputLayout};
use windows::core::Result as HResult;
use windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT;

pub struct StateBackup<'ctx> {
    context: &'ctx ID3D11DeviceContext,
    ia: IABackup<'ctx>
}

#[derive(Default)]
struct IABackup<'ctx> {
    input_layout: Option<ID3D11InputLayout>,
    index_buffer: (Option<ID3D11Buffer>, DXGI_FORMAT, u32),
    primitive_topology: D3D_PRIMITIVE_TOPOLOGY,
    vertex_buffers: [Option<ID3D11Buffer>; D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT as usize],
    vertex_buffer_strides: [u32; D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT as usize],
    vertex_buffer_offs: [u32; D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT as usize],
    _parent: PhantomData<&'ctx ID3D11DeviceContext>
}

impl <'ctx> IABackup<'ctx> {
    unsafe fn backup(context: &'ctx ID3D11DeviceContext) {
        let mut backup : IABackup<'ctx> = Default::default();
        context.IAGetInputLayout(&mut backup.input_layout);
        context.IAGetIndexBuffer(&mut backup.index_buffer.0, &mut backup.index_buffer.1, &mut backup.index_buffer.2);
        context.IAGetPrimitiveTopology(&mut backup.primitive_topology);

        // Blocking on https://github.com/microsoft/windows-rs/issues/1567 to do this without transmute..
        context.IAGetVertexBuffers(0, D3D11_IA_VERTEX_INPUT_RESOURCE_SLOT_COUNT,
                                   &mut std::mem::transmute_copy(&backup.vertex_buffers),
                                   &mut std::mem::transmute_copy(&backup.vertex_buffer_strides),
                                   &mut std::mem::transmute_copy(&backup.vertex_buffer_offs));


    }
}

