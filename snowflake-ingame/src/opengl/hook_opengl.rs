use std::error::Error;
use std::ffi::{c_void, CString};

use detour::static_detour;
use windows::core::{HRESULT, HSTRING};
use windows::Win32::Foundation::{BOOL, GetLastError, PSTR};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::OpenGL::wglGetProcAddress;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

use opengl_bindings::load_with as gl_load_with;

use crate::HookHandle;
use crate::hook_define;
use crate::hook_impl_fn;
use crate::hook_link_chain;

unsafe fn create_wgl_loader() -> Result<impl Fn(&'static str) -> *const c_void, Box<dyn Error>> {
    let opengl_instance = GetModuleHandleA(PSTR(b"opengl32\0".as_ptr()));
    if opengl_instance.is_invalid() {
        let error = GetLastError();
        return Err(windows::core::Error::new(HRESULT(error.0 as i32), HSTRING::new())
            .into());
    }
    Ok(Box::new(move |s| {
        // The source of this string is a &str, so it is always valid UTF-8.
        let proc_name = CString::new(s).unwrap_unchecked();

        if let Some(exported_addr) =  GetProcAddress(opengl_instance, PSTR(proc_name.as_ptr() as *const u8)) {
            return exported_addr as * const c_void
        }

        if let Some(exported_addr) = wglGetProcAddress(PSTR(proc_name.as_ptr() as *const u8)) {
            return exported_addr as * const c_void
        }
        std::ptr::null()
    }))
}


pub struct OpenGLHookContext;

pub struct OpenGLHookHandle {
    swap_buffers_handle: usize
}
static_detour! {
    static SWAP_BUFFERS_DETOUR: extern "system" fn(windows::Win32::Graphics::Gdi::HDC) -> windows::Win32::Foundation::BOOL;
}

pub type FnSwapBuffersHook = Box<dyn (Fn(HDC, SwapBuffersContext) -> BOOL) + Send + Sync>;
hook_define!(chain SWAP_BUFFERS_CHAIN with FnSwapBuffersHook => SwapBuffersContext);

impl OpenGLHookContext {
    hook_impl_fn!(fn swap_buffers(hdc: HDC) -> BOOL =>
        (SWAP_BUFFERS_CHAIN, SWAP_BUFFERS_DETOUR, SwapBuffersContext)
    );

    pub fn init() -> Result<OpenGLHookContext, Box<dyn Error>> {
        let gl_gpa = unsafe { create_wgl_loader()? };

        // Setup call chain termination before detouring
        hook_link_chain! {
            box link SWAP_BUFFERS_CHAIN with SWAP_BUFFERS_DETOUR  => hdc;
        }

        unsafe {
            SWAP_BUFFERS_DETOUR.initialize(std::mem::transmute(gl_gpa("wglSwapBuffers")),
                                           OpenGLHookContext::swap_buffers)?.enable()?;
        }

        // initialize OpenGL context
        gl_load_with(gl_gpa);
        Ok(OpenGLHookContext)
    }

    pub fn new(
        &self,
        swap_buffers: FnSwapBuffersHook,
    ) -> Result<OpenGLHookHandle, Box<dyn Error>> {
        let key = &*swap_buffers as *const _ as *const () as usize;
        SWAP_BUFFERS_CHAIN
            .write()?
            .insert(key, swap_buffers);

        Ok(OpenGLHookHandle {
            swap_buffers_handle: key,
        })
    }
}

impl HookHandle for OpenGLHookHandle {}

impl Drop for OpenGLHookHandle {
    fn drop(&mut self) {
        SWAP_BUFFERS_CHAIN.write().unwrap().remove(&self.swap_buffers_handle);
    }
}

