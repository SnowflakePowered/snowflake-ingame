use imgui::TextureId;
use windows::core::Interface;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0, D3D11_TEX2D_SRV, ID3D11Device1,
    ID3D11ShaderResourceView, ID3D11Texture2D,
};
use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;

use imgui_renderer_dx11::ImguiTexture;

use crate::common::{Dimensions, RenderError};
use crate::ipc::cmd::OverlayTextureEventParams;
use crate::win32::handle::{HandleError, try_close_handle, try_duplicate_handle};

pub(in crate::d3d11) struct Direct3D11Overlay {
    keyed_mutex: Option<IDXGIKeyedMutex>,
    shader_resource_view: Option<ID3D11ShaderResourceView>,
    texture: Option<ID3D11Texture2D>,
    handle: HANDLE,
    window: HWND,
    dimensions: Dimensions,
}

// SAFETY: An instance of Direct3D11Overlay must only
// ever be called  within IDXGISwapChain::Present or IDXGISwapChain::ResizeBuffers
unsafe impl Send for Direct3D11Overlay {}
unsafe impl Sync for Direct3D11Overlay {}

pub(in crate::d3d11) struct KeyedMutexHandle(IDXGIKeyedMutex, u64);
impl Drop for KeyedMutexHandle {
    fn drop(&mut self) {
        unsafe {
            self.0.ReleaseSync(self.1).unwrap_or(());
        }
    }
}

impl KeyedMutexHandle {
    pub fn new(kmt: &IDXGIKeyedMutex, key: u64, ms: u32) -> Option<Self> {
        let kmt = kmt.clone();
        unsafe {
            kmt.AcquireSync(key, ms)
                .map(|_| Some(KeyedMutexHandle(kmt, 0)))
                .unwrap_or(None)
        }
    }
}

impl Direct3D11Overlay {
    #[inline]
    pub fn ready_to_initialize(&self) -> bool {
        self.handle != HANDLE(0)
    }

    #[inline]
    const fn ready_to_paint(&self) -> bool {
        self.shader_resource_view.is_some() && self.keyed_mutex.is_some() && self.texture.is_some()
    }

    pub fn new() -> Direct3D11Overlay {
        Direct3D11Overlay {
            keyed_mutex: None,
            shader_resource_view: None,
            texture: None,
            handle: HANDLE::default(),
            window: HWND::default(),
            dimensions: Dimensions::new(0, 0),
        }
    }

    #[inline]
    pub fn size_matches_viewpoint(&self, size: &Dimensions) -> bool {
        self.dimensions == *size
    }

    pub fn acquire_sync(&self) -> Option<KeyedMutexHandle> {
        if let Some(kmt) = &self.keyed_mutex {
            KeyedMutexHandle::new(kmt, 0, u32::MAX)
        } else {
            None
        }
    }

    fn invalidate(&mut self) {
        self.shader_resource_view = None;
        self.texture = None;
        self.keyed_mutex = None;
    }

    #[must_use]
    pub fn prepare_paint(&mut self, device: ID3D11Device1, output_window: HWND) -> Result<(), RenderError> {
        if self.ready_to_paint() && self.window == output_window {
            return Ok(());
        }

        self.invalidate();

        let tex_2d: ID3D11Texture2D = unsafe { device.OpenSharedResource1(self.handle) }
            .map_err(|e| RenderError::OverlayHandleError(self.handle, e))?;


        let tex_mtx: IDXGIKeyedMutex = Interface::cast(&tex_2d)?;

        let mut tex_desc = Default::default();
        unsafe {
            tex_2d.GetDesc(&mut tex_desc);
        }

        let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: tex_desc.Format,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_SRV {
                    MipLevels: tex_desc.MipLevels,
                    MostDetailedMip: 0,
                },
            },
        };

        let srv = unsafe { device.CreateShaderResourceView(&tex_2d, &srv_desc)? };

        self.keyed_mutex = Some(tex_mtx);
        self.texture = Some(tex_2d);
        self.dimensions = Dimensions::new(tex_desc.Width, tex_desc.Height);

        self.shader_resource_view = Some(srv);
        self.window = output_window;

        eprintln!("[dx11] success on overlay");
        Ok(())
    }

    #[must_use]
    pub fn refresh(&mut self, params: OverlayTextureEventParams) -> Result<(), HandleError> {
        let owning_pid = params.source_pid;
        let handle = HANDLE(params.handle as isize);

        self.handle = {
            let duped_handle = try_duplicate_handle(owning_pid as u32, handle)?;

            // this doesn't do anything if its already null.
            self.invalidate();

            if self.ready_to_initialize() {
                try_close_handle(self.handle)?;
            }

            eprintln!("[dx11] duped handle {:x?}", duped_handle);
            duped_handle
        };
        Ok(())
    }

    pub fn paint<F: Sized + FnOnce(TextureId, Dimensions)>(&self, f: F) {
        if let Some(srv_handle) = &self.shader_resource_view {
            f(srv_handle.as_tex_id(), self.dimensions);
        }
    }
}
