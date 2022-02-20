use std::error::Error;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory2, DXGI_CREATE_FACTORY_DEBUG, IDXGIFactory2};

fn main() -> Result<(), Box<dyn Error>>{
    println!("Hello, world!");
    let factory : IDXGIFactory2 = unsafe {
        CreateDXGIFactory2::<IDXGIFactory2>(DXGI_CREATE_FACTORY_DEBUG)?
    };
    Ok(())
}
