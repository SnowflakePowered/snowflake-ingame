use windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC;

#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct Dimensions {
    pub width: u32,
    pub height: u32,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Dimensions {
        Dimensions { width, height }
    }
}

impl From<[f32; 2]> for Dimensions {
    fn from(item: [f32; 2]) -> Self {
        Dimensions {
            width: item[0] as u32,
            height: item[1] as u32
        }
    }
}

impl From<Dimensions> for [f32; 2] {
    fn from(item: Dimensions) -> Self {
        [item.width as f32, item.height as f32]
    }
}

impl From<D3D11_TEXTURE2D_DESC> for Dimensions {
    fn from(item: D3D11_TEXTURE2D_DESC) -> Self {
        Dimensions {
            width: item.Width,
            height: item.Height
        }
    }
}

