use std::error::Error;
use std::ffi::{c_void, CString};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicIsize, Ordering};
use parking_lot::{RwLock, RwLockWriteGuard};
use windows::core::{HRESULT, HSTRING, PCSTR};
use windows::Win32::Foundation::GetLastError;
use windows::Win32::Graphics::Gdi::{HDC, WindowFromDC};
use windows::Win32::Graphics::OpenGL::{HGLRC, wglGetCurrentContext, wglGetProcAddress};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use imgui_renderer_ogl::OpenGLImguiRenderer;
use opengl_bindings::Gl;
use crate::common::{Dimensions, OverlayWindow};
use crate::hook::{HookHandle, HookChain};
use crate::ipc::IpcHandle;
use crate::wgl::hook_wgl::{FnSwapBuffersHook, WGLHookContext};
use crate::wgl::imgui_wgl::WGLImguiController;

unsafe fn create_wgl_loader() -> Result<impl Fn(&'static str) -> *const c_void, Box<dyn Error>> {
    let opengl_instance = GetModuleHandleA(PCSTR(b"opengl32\0".as_ptr()));
    if opengl_instance.is_invalid() {
        let error = GetLastError();
        return Err(windows::core::Error::new(HRESULT(error.0 as i32), HSTRING::new()).into());
    }

    let local_gpa = if let Some(local_gpa) = GetProcAddress(opengl_instance, PCSTR(b"wglGetProcAddress\0".as_ptr())) {
        local_gpa
    } else {
        let error = GetLastError();
        return Err(windows::core::Error::new(HRESULT(error.0 as i32), HSTRING::new()).into());
    };

    // let local_gpa = std::mem::transmute::<_, fn(*const u8) -> *const c_void>(local_gpa);

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

// this is so bad...
struct OwnedGl(Gl);
unsafe impl Send for OwnedGl {}
unsafe impl Sync for OwnedGl {}

impl Deref for OwnedGl {
    type Target = Gl;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct WGLKernel {
    gl: Arc<RwLock<OwnedGl>>,
    hook: WGLHookContext,
    ipc: IpcHandle,
    imgui: Arc<RwLock<WGLImguiController>>,
    ctx: Arc<AtomicIsize>,
}

impl WGLKernel {
    pub fn new(ipc: IpcHandle) -> Result<Self, Box<dyn Error>> {
        let gl_gpa = unsafe { create_wgl_loader()? };
        let swap_buffers = unsafe { std::mem::transmute(gl_gpa("wglSwapBuffers")) };
        let gl = Gl::load_with(gl_gpa);

        let imgui = Arc::new(RwLock::new(WGLImguiController::new()));
        Ok(WGLKernel {
            hook: WGLHookContext::init(swap_buffers)?,
            gl: Arc::new(RwLock::new(OwnedGl(gl))),
            imgui,
            ipc,
            ctx: Arc::new(AtomicIsize::new(0)),
        })
    }

    unsafe fn swapbuffers_impl(
        gl: &Gl,
        handle: IpcHandle,
        hdc: HDC,
        hglrc: HGLRC,
        mut imgui: RwLockWriteGuard<WGLImguiController>,
    ) {
        let window = WindowFromDC(hdc);
        let mut client_rect = Default::default();
        GetClientRect(window, &mut client_rect);

        let size = Dimensions {
            width: client_rect.right.abs_diff(client_rect.left),
            height: client_rect.bottom.abs_diff(client_rect.top),
        };

        if !imgui.prepare_paint(gl, window, hglrc, size) {
            eprintln!("[ogl] Failed to setup imgui render state");
            return;
        }

        imgui.frame(|ctx, render| {
            let ui = ctx.frame();
            ui.show_metrics_window(&mut false);
            ui.show_demo_window(&mut false);
            render.render(ui.render()).unwrap()
        });
    }

    pub fn make_swap_buffers(&self) -> FnSwapBuffersHook {
        let handle = self.ipc.clone();
        let imgui = self.imgui.clone();
        let gl = self.gl.clone();
        let ctx = self.ctx.clone();
        Box::new(move |hdc, mut next| {
            let hglrc = unsafe { wglGetCurrentContext() };
            unsafe {
                let old_ctx = ctx.swap(hglrc.0, Ordering::SeqCst);
                if old_ctx != hglrc.0 {
                    let gl_gpa = create_wgl_loader().unwrap();
                    let mut new_gl =  OwnedGl(Gl::load_with(gl_gpa));
                    std::mem::swap(gl.write().deref_mut(), &mut new_gl);
                }
            }

            let handle = handle.clone();
            let gl = gl.clone();

            unsafe { WGLKernel::swapbuffers_impl(&gl.read(), handle, hdc, hglrc,imgui.write()); }
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