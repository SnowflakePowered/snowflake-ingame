use std::marker::PhantomData;
use windows::Win32::Graphics::Direct3D11::{D3D11_BIND_INDEX_BUFFER, D3D11_BIND_VERTEX_BUFFER,
                                           D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_WRITE,
                                           D3D11_USAGE_DYNAMIC, ID3D11Buffer, ID3D11Device};
use windows::core::Result as HResult;

const VERTEX_BUF_ADD_CAPACITY: usize = 5000;
const INDEX_BUF_ADD_CAPACITY: usize = 10000;

#[derive(Debug)]
pub struct ImGuiBuffer<T, const ADD_CAPACITY: usize, const BIND_FLAGS: u32> {
    buffer: ID3D11Buffer,
    len: usize,
    device: ID3D11Device,
    _draw_ty: PhantomData<T>
}

impl <T, const ADD_CAPACITY: usize, const BIND_FLAGS: u32> ImGuiBuffer<T, ADD_CAPACITY, BIND_FLAGS> {
    pub fn new(device: &ID3D11Device) -> HResult<Self> {
        let (buffer, len) = Self::create_buffer(device, 0)?;
        Ok(ImGuiBuffer {
            device: device.clone(), // this should be fine, addref is cheap.
            buffer,
            len,
            _draw_ty: PhantomData
        })
    }

    pub fn reserve(&mut self, count: usize) -> HResult<()> {
        if self.len() < count {
            let (buffer, len) =
                Self::create_buffer(&self.device, count)?;
            self.buffer = buffer;
            self.len = len;
        }
        Ok(())
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    fn create_buffer(device: &ID3D11Device, count: usize) -> HResult<(ID3D11Buffer, usize)> {
        let len = count + ADD_CAPACITY;
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: (len * std::mem::size_of::<T>()) as u32,
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: BIND_FLAGS,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0,
            MiscFlags: 0,
            StructureByteStride: 0,
        };
        let buffer = unsafe { device.CreateBuffer(&desc, std::ptr::null())? };
        Ok((buffer, len))
    }
}

pub type VertexBuffer = ImGuiBuffer<imgui::DrawVert, VERTEX_BUF_ADD_CAPACITY, {D3D11_BIND_VERTEX_BUFFER.0}>;
pub type IndexBuffer = ImGuiBuffer<imgui::DrawIdx, INDEX_BUF_ADD_CAPACITY, {D3D11_BIND_INDEX_BUFFER.0}>;
