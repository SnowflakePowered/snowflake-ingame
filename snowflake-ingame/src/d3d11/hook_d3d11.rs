use std::error::Error;

use crate::hook::HookHandle;
use detour::static_detour;
use windows::core::{Vtable, HSTRING};
use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device_Vtbl, D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_MODE_SCALING_UNSPECIFIED,
    DXGI_MODE_SCANLINE_ORDER_UNSPECIFIED, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory, IDXGIFactory, IDXGISwapChain, IDXGISwapChain_Vtbl, DXGI_ERROR_INVALID_CALL,
    DXGI_SWAP_CHAIN_DESC, DXGI_SWAP_EFFECT_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
};

use crate::hook_define;
use crate::hook_impl_fn;
use crate::hook_key;
use crate::hook_link_chain;

struct VTables {
    pub vtbl_dxgi_swapchain: *const IDXGISwapChain_Vtbl,
    #[allow(dead_code)]
    pub vtbl_d3d11_device: *const ID3D11Device_Vtbl,
}

fn get_vtables() -> Result<VTables, Box<dyn Error>> {
    let wnd = crate::win32::window::TempWindow::new(b"snowflake_ingame_d3d\0");

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
        OutputWindow: (&wnd).into(),
        Windowed: BOOL(1),
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: 0,
    };

    let feature_levels = vec![D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1];
    let mut out_device = None;
    let mut _out_context = None;
    let mut _out_feature_level = D3D_FEATURE_LEVEL_11_0;

    let fac: IDXGIFactory = unsafe { CreateDXGIFactory()? };
    eprintln!("[dx11] factory created");

    let _dev = unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HINSTANCE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut out_device),
            Some(&mut _out_feature_level),
            Some(&mut _out_context),
        )?
    };
    let device = out_device.ok_or(windows::core::Error::new(
        DXGI_ERROR_INVALID_CALL,
        HSTRING::default(),
    ))?;
    eprintln!("[dx11] device created");

    let swap_chain: IDXGISwapChain = unsafe {
        let mut swap_chain = None;
        fac.CreateSwapChain(&device, &swapchain_desc, &mut swap_chain)
            .ok()?;
        swap_chain.expect("[dx11] swapchain creation failed.")
    };

    eprintln!("[dx11] swapchain acquired");
    unsafe {
        Ok(VTables {
            vtbl_dxgi_swapchain: swap_chain.vtable(),
            vtbl_d3d11_device: device.vtable(),
        })
    }
}

pub type FnPresentHook =
    Box<dyn Send + Sync + Fn(IDXGISwapChain, u32, u32, PresentContext) -> windows::core::HRESULT>;

pub type FnResizeBuffersHook = Box<
    dyn Send
        + Sync
        + Fn(
            IDXGISwapChain,
            u32,
            u32,
            u32,
            DXGI_FORMAT,
            u32,
            ResizeBuffersContext,
        ) -> windows::core::HRESULT,
>;

static_detour! {
    static PRESENT_DETOUR: extern "system" fn(IDXGISwapChain, u32, u32) -> windows::core::HRESULT;
    static RESIZE_BUFFERS_DETOUR: extern "system" fn(IDXGISwapChain, u32, u32, u32, DXGI_FORMAT, u32) -> windows::core::HRESULT;
}

struct Direct3D11HookHandle {
    present_handle: usize,
    resize_buffers_handle: usize,
}

hook_define!(chain PRESENT_CHAIN with FnPresentHook => PresentContext);
hook_define!(chain RESIZE_BUFFERS_CHAIN with FnResizeBuffersHook => ResizeBuffersContext);

pub struct Direct3D11HookContext;

impl Direct3D11HookContext {
    hook_impl_fn!(fn present(this: IDXGISwapChain, syncinterval: u32, flags: u32) -> windows::core::HRESULT
        => (PRESENT_CHAIN, PRESENT_DETOUR, PresentContext));
    hook_impl_fn!(fn resize_buffers(this: IDXGISwapChain,  bufcount: u32, width: u32, height: u32, format: DXGI_FORMAT, swapchain_flags: u32) -> windows::core::HRESULT
        => (RESIZE_BUFFERS_CHAIN, RESIZE_BUFFERS_DETOUR, ResizeBuffersContext));

    pub fn init() -> Result<Direct3D11HookContext, Box<dyn Error>> {
        let vtables = get_vtables()?;

        // Setup call chain termination before detouring
        hook_link_chain! {
            box link PRESENT_CHAIN with PRESENT_DETOUR => this, sync, flags;
        }

        hook_link_chain! {
            box link RESIZE_BUFFERS_CHAIN with RESIZE_BUFFERS_DETOUR => this, count, width, height, format, flags;
        }

        unsafe {
            PRESENT_DETOUR
                .initialize(
                    std::mem::transmute((*vtables.vtbl_dxgi_swapchain).Present),
                    Direct3D11HookContext::present,
                )?
                .enable()?;
            RESIZE_BUFFERS_DETOUR
                .initialize(
                    std::mem::transmute((*vtables.vtbl_dxgi_swapchain).ResizeBuffers),
                    Direct3D11HookContext::resize_buffers,
                )?
                .enable()?;
        }

        Ok(Direct3D11HookContext)
    }

    pub fn new(
        &self,
        present: FnPresentHook,
        resize_buffers: FnResizeBuffersHook,
    ) -> Result<impl HookHandle, Box<dyn Error>> {
        let present_key = hook_key!(box present);
        let resize_key = hook_key!(box resize_buffers);
        PRESENT_CHAIN.write()?.insert(present_key, present);

        RESIZE_BUFFERS_CHAIN
            .write()?
            .insert(resize_key, resize_buffers);

        Ok(Direct3D11HookHandle {
            present_handle: present_key,
            resize_buffers_handle: resize_key,
        })
    }
}

impl HookHandle for Direct3D11HookHandle {}

impl Drop for Direct3D11HookHandle {
    fn drop(&mut self) {
        PRESENT_CHAIN.write().unwrap().remove(&self.present_handle);
        RESIZE_BUFFERS_CHAIN
            .write()
            .unwrap()
            .remove(&self.resize_buffers_handle);
    }
}
