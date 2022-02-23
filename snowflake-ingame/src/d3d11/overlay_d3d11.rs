use windows::Win32::Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, HWND};
use windows::Win32::Graphics::Direct3D11::{ID3D11ShaderResourceView, ID3D11Texture2D};
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

use crate::ipc::cmd::{OverlayTextureEventParams, Size};

pub struct D3D11Overlay {
    keyed_mutex: Option<IDXGIKeyedMutex>,
    shader_resource_view: Option<ID3D11ShaderResourceView>,
    texture: Option<ID3D11Texture2D>,
    handle: HANDLE,
    window: HWND,
    size: Size,
    ready_to_paint: bool
}

impl D3D11Overlay {

    pub fn new() -> D3D11Overlay {
        D3D11Overlay {
            keyed_mutex: None,
            shader_resource_view: None,
            texture: None,
            handle: HANDLE::default(),
            window: HWND::default(),
            size: Size::new(0, 0),
            ready_to_paint: false
        }
    }

    pub fn size_matches_viewpoint(&self, size: &Size) -> bool {
        return self.size == *size;
    }

    pub fn acquire_sync(&mut self) -> bool {
        if let Some(kmt) = &self.keyed_mutex {
            unsafe { kmt.AcquireSync(0, u32::MAX).map(|_| true).unwrap_or(false) }
        } else {
            false
        }
    }

    pub fn release_sync(&mut self) {
        if let Some(kmt) = &self.keyed_mutex {
            unsafe { kmt.ReleaseSync(0).unwrap_or(()); }
        }
    }

    pub fn invalidate(&mut self) {
        self.shader_resource_view = None;
        self.texture = None;
        self.keyed_mutex = None;
        self.ready_to_paint = false;
    }

    // pub fn prepare_paint(device: ID3D11Device1) -> bool {
    //     // device.CreateShaderResourceView()
    // }

    pub fn refresh(&mut self, params: OverlayTextureEventParams) -> bool {
        let owning_pid = params.source_pid;
        let handle = HANDLE(params.handle as isize);

        self.handle = unsafe {
            let process = OpenProcess(PROCESS_DUP_HANDLE, false, owning_pid as u32);
            if process.is_invalid() {
                eprintln!("unable to open source process");
                return false
            }

            let mut duped_handle = std::ptr::null_mut();
            if !DuplicateHandle(process, handle, GetCurrentProcess(), duped_handle,
            0, false, DUPLICATE_SAME_ACCESS).as_bool() {
                eprintln!("unable to duplicate handle");
                return false
            }

            // this doesn't do anything if its already null.
            self.invalidate();

            CloseHandle(self.handle);

            eprintln!("duped handle {:p}", duped_handle);
            *duped_handle
        };
        true
    }

    pub fn paint<F: Sized + FnOnce(usize, u32, u32)>(&self, f: F) {
        // addref
        if let Some(srv_handle) = &self.shader_resource_view {
            unsafe {
                f(std::mem::transmute_copy(&srv_handle), 0, 0);
            }
            // release
        }
    }
}