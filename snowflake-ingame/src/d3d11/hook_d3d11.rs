use std::error::Error;
use std::iter;
use std::lazy::SyncLazy;
use std::sync::RwLock;

use detour::static_detour;
use indexmap::IndexMap;
use windows::core::{HRESULT, Interface};
use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CREATE_DEVICE_FLAG,
    D3D11_SDK_VERSION, D3D11CreateDeviceAndSwapChain,
    ID3D11Device_Vtbl,
};
use windows::Win32::Graphics::Dxgi::{
    DXGI_SWAP_CHAIN_DESC,
    DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    IDXGISwapChain,
    IDXGISwapChain_Vtbl,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC,
    DXGI_MODE_SCALING_UNSPECIFIED, DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_RATIONAL,
    DXGI_SAMPLE_DESC,
};

use crate::hook::HookChain;

use crate::{HookHandle, win32};

struct VTables {
    pub vtbl_dxgi_swapchain: *const IDXGISwapChain_Vtbl,
    #[allow(dead_code)]
    pub vtbl_d3d11_device: *const ID3D11Device_Vtbl,
}

fn get_vtables() -> Result<VTables, Box<dyn Error>> {
    let wnd = win32::window::TempWindow::new(b"snowflake_ingame_d3d\0");

    let swapchain_desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC {
            Width: 256,
            Height: 256,
            RefreshRate: DXGI_RATIONAL {
                Numerator: 60,
                Denominator: 1,
            },
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            ScanlineOrdering: DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED,
            Scaling: DXGI_MODE_SCALING_UNSPECIFIED,
        },
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 1,
        OutputWindow: wnd.into(),
        Windowed: BOOL(1),
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: 0,
    };

    let feature_levels = vec![D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1];
    let mut out_swapchain = None;
    let mut out_device = None;
    let mut out_context = None;
    let mut out_feature_level = D3D_FEATURE_LEVEL_11_0;

    let _res = unsafe {
        D3D11CreateDeviceAndSwapChain(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HINSTANCE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            feature_levels.as_ptr(),
            feature_levels.len() as u32,
            D3D11_SDK_VERSION,
            &swapchain_desc,
            &mut out_swapchain,
            &mut out_device,
            &mut out_feature_level,
            &mut out_context,
        )?
    };

    unsafe {
        let swap_chain = out_swapchain.unwrap();
        let device = out_device.unwrap();
        Ok(VTables {
            vtbl_dxgi_swapchain: Interface::vtable(&swap_chain),
            vtbl_d3d11_device: Interface::vtable(&device),
        })
    }
}

pub struct PresentContext<'a> {
    chain: iter::Rev<indexmap::map::Iter<'a, usize, FnPresentHook>>,
}

impl<'a> HookChain<'a, FnPresentHook> for PresentContext<'a> {
    fn fp_next(&mut self) -> &'a FnPresentHook {
        let (_, fp) = unsafe { self.chain.next().unwrap_unchecked() };
        fp
    }
}

pub struct ResizeBuffersContext<'a> {
    chain: iter::Rev<indexmap::map::Iter<'a, usize, FnResizeBuffersHook>>,
}

impl<'a> HookChain<'a, FnResizeBuffersHook> for ResizeBuffersContext<'a> {
    fn fp_next(&mut self) -> &'a FnResizeBuffersHook {
        let (_, fp) = unsafe { self.chain.next().unwrap_unchecked() };
        fp
    }
}

pub type FnPresentHook = fn(
    this: IDXGISwapChain,
    syncinterval: u32,
    flags: u32,
    next: PresentContext,
) -> windows::core::HRESULT;
pub type FnResizeBuffersHook = fn(
    this: IDXGISwapChain,
    buffercount: u32,
    width: u32,
    height: u32,
    new_format: DXGI_FORMAT,
    swapchain_flags: u32,
    next: ResizeBuffersContext,
) -> windows::core::HRESULT;

static_detour! {
    static PRESENT_DETOUR: extern "system" fn(IDXGISwapChain, u32, u32) -> windows::core::HRESULT;
    static RESIZE_BUFFERS_DETOUR: extern "system" fn(IDXGISwapChain, u32, u32, u32, DXGI_FORMAT, u32) -> windows::core::HRESULT;
}

static PRESENT_CHAIN: SyncLazy<RwLock<IndexMap<usize, FnPresentHook>>> =
    SyncLazy::new(|| RwLock::new(IndexMap::new()));
static RESIZE_BUFFERS_CHAIN: SyncLazy<RwLock<IndexMap<usize, FnResizeBuffersHook>>> =
    SyncLazy::new(|| RwLock::new(IndexMap::new()));

pub struct D3D11HookHandle {
    present_handle: usize,
    resize_buffers_handle: usize,
}

pub struct D3D11HookContext;

impl D3D11HookContext {
    fn present(this: IDXGISwapChain, syncinterval: u32, flags: u32) -> HRESULT {
        if let Ok(chain) = PRESENT_CHAIN.read() {
            if let Some((_, next)) = chain.last() {
                let mut iter = chain.iter().rev();

                // Advance the chain to the next call.
                iter.next();
                return next(this, syncinterval, flags, PresentContext { chain: iter });
            }
        }
        PRESENT_DETOUR.call(this, syncinterval, flags)
    }

    fn resize_buffers(
        this: IDXGISwapChain,
        bufcount: u32,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
        swapchain_flags: u32,
    ) -> HRESULT {
        if let Ok(chain) = RESIZE_BUFFERS_CHAIN.read() {
            if let Some((_, next)) = chain.last() {
                let mut iter = chain.iter().rev();

                // Advance the chain to the next call.
                iter.next();
                return next(
                    this,
                    bufcount,
                    width,
                    height,
                    format,
                    swapchain_flags,
                    ResizeBuffersContext { chain: iter },
                );
            }
        }
        RESIZE_BUFFERS_DETOUR.call(this, bufcount, width, height, format, swapchain_flags)
    }

    pub fn init() -> Result<D3D11HookContext, Box<dyn Error>> {
        let vtables = get_vtables()?;
        unsafe {
            PRESENT_DETOUR
                .initialize(
                    std::mem::transmute((*vtables.vtbl_dxgi_swapchain).Present),
                    D3D11HookContext::present,
                )?
                .enable()?;
            RESIZE_BUFFERS_DETOUR
                .initialize(
                    std::mem::transmute((*vtables.vtbl_dxgi_swapchain).ResizeBuffers),
                    D3D11HookContext::resize_buffers,
                )?
                .enable()?;
        }

        PRESENT_CHAIN
            .write()?
            .insert(0, |this, sync, flags, mut _next| {
                        PRESENT_DETOUR.call(this, sync, flags)
            });

        RESIZE_BUFFERS_CHAIN.write()?.insert(
            0,
            |this, count, width, height, format, flags, mut _next| {
                RESIZE_BUFFERS_DETOUR.call(this, count, width, height, format, flags)
            },
        );

        Ok(D3D11HookContext)
    }

    pub fn new(
        &self,
        present: FnPresentHook,
        resize_buffers: FnResizeBuffersHook,
    ) -> Result<D3D11HookHandle, Box<dyn Error>> {
        PRESENT_CHAIN
            .write()?
            .insert(present as *const () as usize, present);
        RESIZE_BUFFERS_CHAIN
            .write()?
            .insert(resize_buffers as *const () as usize, resize_buffers);

        Ok(D3D11HookHandle {
            present_handle: present as *const () as usize,
            resize_buffers_handle: resize_buffers as *const () as usize,
        })
    }
}
impl HookHandle for D3D11HookHandle {}

impl Drop for D3D11HookHandle {
    fn drop(&mut self) {
        PRESENT_CHAIN.write().unwrap().remove(&self.present_handle);
        RESIZE_BUFFERS_CHAIN
            .write()
            .unwrap()
            .remove(&self.resize_buffers_handle);
    }
}
