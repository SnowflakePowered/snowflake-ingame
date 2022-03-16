use std::error::Error;
use std::ffi::{c_void, CString};
use std::mem::ManuallyDrop;
use std::sync::Arc;
use parking_lot::RwLock;
use windows::core::{HRESULT, HSTRING, PCSTR};
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::OpenGL::wglGetProcAddress;
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use imgui_renderer_ogl::OpenGLImguiRenderer;
use opengl_bindings::Gl;
use crate::hook::{HookHandle, HookChain};
use crate::ipc::IpcHandle;
use crate::wgl::hook_wgl::{FnSwapBuffersHook, WGLHookContext};

unsafe fn create_wgl_loader() -> Result<impl Fn(&'static str) -> *const c_void, Box<dyn Error>> {
    let opengl_instance = GetModuleHandleA(PCSTR(b"opengl32\0".as_ptr()));
    if opengl_instance.is_invalid() {
        let error = GetLastError();
        return Err(windows::core::Error::new(HRESULT(error.0 as i32), HSTRING::new()).into());
    }
    Ok(move |s| {
        // The source of this string is a &str, so it is always valid UTF-8.
        let proc_name = CString::new(s).unwrap_unchecked();

        if let Some(exported_addr) =
        GetProcAddress(opengl_instance, PCSTR(proc_name.as_ptr() as *const u8))
        {
            return exported_addr as *const c_void;
        }

        if let Some(exported_addr) = wglGetProcAddress(PCSTR(proc_name.as_ptr() as *const u8)) {
            return exported_addr as *const c_void;
        }
        std::ptr::null()
    })
}

pub struct WGLKernel {
    gl: Gl,
    hook: WGLHookContext,
    ipc: IpcHandle,
    overlay: Arc<RwLock<OpenGLImguiRenderer>>
}

impl WGLKernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        let gl_gpa = unsafe { create_wgl_loader()? };
        let swap_buffers = unsafe { std::mem::transmute(gl_gpa("wglSwapBuffers")) };
        let gl = Gl::load_with(gl_gpa);
        let mut ctx = imgui::Context::create();
        let overlay = Arc::new(RwLock::new(OpenGLImguiRenderer::new(&gl, &mut ctx)?));
        Ok(WGLKernel {
            hook: WGLHookContext::init(swap_buffers)?,
            gl,
            overlay,
            ipc
        })
    }

    unsafe fn swapbuffers_impl(
        handle: IpcHandle,
        hdc: HDC,
    ) {
        eprintln!("[wgl] swapbuffer")
    }

    pub fn make_swap_buffers(&self) -> FnSwapBuffersHook {
        let handle = self.ipc.clone();
        Box::new(move |hdc, mut next| {
            let handle = handle.clone();
            unsafe { WGLKernel::swapbuffers_impl(handle, hdc); }
            let fp = next.fp_next();
            fp(hdc, next)
        })
    }

    pub fn init(&mut self) -> Result<ManuallyDrop<impl HookHandle>, Box<dyn Error>> {
        println!("[wgl] init");
        let handle = self.hook
            .new(self.make_swap_buffers())?
            .persist();

        Ok(handle)
    }
}