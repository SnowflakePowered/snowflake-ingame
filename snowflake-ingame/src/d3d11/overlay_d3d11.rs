use crate::common::Dimensions;
use std::mem::MaybeUninit;
use std::sync::{LockResult, Mutex, MutexGuard};
use windows::core::Interface;
use windows::Win32::Foundation::{
    CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, HANDLE, HWND,
};
use windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D;
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device1, ID3D11ShaderResourceView, ID3D11Texture2D, D3D11_SHADER_RESOURCE_VIEW_DESC,
    D3D11_SHADER_RESOURCE_VIEW_DESC_0, D3D11_TEX2D_SRV, D3D11_TEXTURE2D_DESC,
};
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

use crate::ipc::cmd::OverlayTextureEventParams;

pub struct D3D11Overlay {
    keyed_mutex: Option<IDXGIKeyedMutex>,
    shader_resource_view: Option<ID3D11ShaderResourceView>,
    texture: Option<ID3D11Texture2D>,
    handle: HANDLE,
    window: HWND,
    size: Dimensions,
    ready_to_paint: bool,
}

/* this is hilariously unsafe and probably unsound */
unsafe impl Send for D3D11Overlay {}
unsafe impl Sync for D3D11Overlay {}

impl D3D11Overlay {
    pub fn ready_to_initialize(&self) -> bool {
        self.handle != HANDLE(0)
    }

    pub fn new() -> D3D11Overlay {
        D3D11Overlay {
            keyed_mutex: None,
            shader_resource_view: None,
            texture: None,
            handle: HANDLE::default(),
            window: HWND::default(),
            size: Dimensions::new(0, 0),
            ready_to_paint: false,
        }
    }

    pub fn size_matches_viewpoint(&self, size: &Dimensions) -> bool {
        return self.size == *size;
    }

    pub fn acquire_sync(&self) -> bool {
        if let Some(kmt) = &self.keyed_mutex {
            unsafe { kmt.AcquireSync(0, u32::MAX).map(|_| true).unwrap_or(false) }
        } else {
            false
        }
    }

    pub fn release_sync(&self) {
        if let Some(kmt) = &self.keyed_mutex {
            unsafe {
                kmt.ReleaseSync(0).unwrap_or(());
            }
        }
    }

    pub fn invalidate(&mut self) {
        self.shader_resource_view = None;
        self.texture = None;
        self.keyed_mutex = None;
        self.ready_to_paint = false;
    }

    // todo: make this err type
    pub fn prepare_paint(&mut self, device: ID3D11Device1, output_window: HWND) -> bool {
        if self.ready_to_paint && self.window == output_window {
            return true;
        }
        self.invalidate();

        let tex_2d: ID3D11Texture2D =
            if let Ok(resource) = unsafe { device.OpenSharedResource1(self.handle) } {
                resource
            } else {
                eprintln!("[dx11] unable to open shared resource {:?}", self.handle);
                return false;
            };

        let tex_mtx: IDXGIKeyedMutex = if let Ok(mtx) = unsafe { Interface::cast(&tex_2d) } {
            mtx
        } else {
            eprintln!("[dx11] unable to open keyed mutex");
            return false;
        };

        let tex_desc = unsafe {
            let mut tex_desc = MaybeUninit::uninit();
            tex_2d.GetDesc(tex_desc.as_mut_ptr());
            tex_desc.assume_init()
        };

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

        let srv = if let Ok(srv) = unsafe { device.CreateShaderResourceView(&tex_2d, &srv_desc) } {
            srv
        } else {
            eprintln!("[dx11] unable to create srv");
            return false;
        };

        self.keyed_mutex = Some(tex_mtx);
        self.texture = Some(tex_2d);
        self.size = Dimensions::new(tex_desc.Width, tex_desc.Height);

        self.shader_resource_view = Some(srv);
        self.window = output_window;
        self.ready_to_paint = true;

        eprintln!("[dx11] success on overlay");
        true
    }

    // todo: make this err type.
    pub fn refresh(&mut self, params: OverlayTextureEventParams) -> bool {
        let owning_pid = params.source_pid;
        let handle = HANDLE(params.handle as isize);

        self.handle = unsafe {
            let process = OpenProcess(PROCESS_DUP_HANDLE, false, owning_pid as u32);
            if process.is_invalid() {
                eprintln!("unable to open source process");
                return false;
            }

            let mut duped_handle = MaybeUninit::uninit();
            if !(DuplicateHandle(
                process,
                handle,
                GetCurrentProcess(),
                duped_handle.as_mut_ptr(),
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
            .as_bool())
            {
                eprintln!("[dx11] unable to duplicate handle");
                return false;
            }

            // this doesn't do anything if its already null.
            self.invalidate();

            if self.ready_to_initialize() && !(CloseHandle(self.handle).as_bool()) {
                return false;
            }

            eprintln!("[dx11] duped handle {:p}", duped_handle.as_ptr());
            duped_handle.assume_init()
        };
        true
    }

    pub fn paint<F: Sized + FnOnce(usize, Dimensions)>(&self, f: F) {
        if let Some(srv_handle) = &self.shader_resource_view {
            unsafe {
                let srv = srv_handle.clone();
                f(std::mem::transmute(srv), self.size);
            }
        }
    }
}
