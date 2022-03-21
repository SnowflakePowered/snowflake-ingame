use std::error::Error;
use std::ffi::{c_void, CString};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};
use parking_lot::{RwLock, RwLockWriteGuard};
use windows::core::{HRESULT, HSTRING, PCSTR};
use windows::Win32::Foundation::{GetLastError, HWND};
use windows::Win32::Graphics::Gdi::{HDC, WindowFromDC};
use windows::Win32::Graphics::OpenGL::{HGLRC, wglGetCurrentContext, wglGetProcAddress};
use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
use imgui_renderer_ogl::RenderToken;
use opengl_bindings::Gl;
use crate::common::{Dimensions, OverlayWindow, RenderError};
use crate::hook::{HookHandle, HookChain};
use crate::ipc::cmd::{GameWindowCommand, GameWindowCommandType};
use crate::ipc::IpcHandle;
use crate::wgl::hook::{FnSwapBuffersHook, WGLHookContext};
use crate::wgl::imgui::WGLImguiController;
use crate::wgl::overlay::WGLOverlay;

use crate::kernel::common::{FrameKernel, KernelContext};
use crate::win32::wndproc::{WndProcHandle};
use windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE;

// this is so bad...
pub(in crate::wgl) struct OwnedGl(Gl);
unsafe impl Send for OwnedGl {}
unsafe impl Sync for OwnedGl {}

impl Deref for OwnedGl {
    type Target = Gl;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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
        GetProcAddress(opengl_instance, PCSTR(proc_name.as_ptr() as *const u8)) {
            return exported_addr as *const c_void;
        }

        if let Some(exported_addr) = wglGetProcAddress(PCSTR(proc_name.as_ptr() as *const u8)) {
            return exported_addr as *const c_void;
        }

        std::ptr::null()
    })
}

pub struct WGLKernel {
    gl: Arc<RwLock<OwnedGl>>,
    hook: WGLHookContext,
    ipc: IpcHandle,
    imgui: Arc<RwLock<WGLImguiController>>,
    overlay: Arc<RwLock<WGLOverlay>>,
    ctx: Arc<AtomicIsize>,
    wp: Arc<RwLock<WndProcHandle>>
}

impl FrameKernel for WGLKernel {
    type Handle = impl HookHandle;

    fn new(context: KernelContext) -> Result<Self, Box<dyn Error>> {
        let KernelContext { ipc, imgui } = context;
        let gl_gpa = unsafe { create_wgl_loader()? };
        let swap_buffers = unsafe { std::mem::transmute(gl_gpa("wglSwapBuffers")) };
        let gl = Gl::load_with(gl_gpa);

        Ok(WGLKernel {
            ipc,
            hook: WGLHookContext::init(swap_buffers)?,
            gl: Arc::new(RwLock::new(OwnedGl(gl))),
            imgui: Arc::new(RwLock::new(WGLImguiController::new(imgui))),
            overlay: Arc::new(RwLock::new(WGLOverlay::new())),
            ctx: Arc::new(AtomicIsize::new(0)),
            wp: Arc::new(RwLock::new(WndProcHandle::new()))
        })
    }

    fn init(&mut self) -> Result<ManuallyDrop<Self::Handle>, Box<dyn Error>> {
        println!("[wgl] init");
        let handle = self.hook
            .new(self.make_swap_buffers())?
            .persist();

        Ok(handle)
    }
}

impl WGLKernel {
    fn swapbuffers_impl(
        gl: &Gl,
        handle: IpcHandle,
        hdc: HDC,
        hglrc: HGLRC,
        mut overlay: RwLockWriteGuard<WGLOverlay>,
        mut imgui: RwLockWriteGuard<WGLImguiController>,
        mut wndproc: RwLockWriteGuard<WndProcHandle>
    ) -> Result<RenderToken, RenderError>{
        // Handle update of any overlay here.
        if let Ok(cmd) = handle.try_recv() {
            match &cmd.ty {
                &GameWindowCommandType::OVERLAY_TEXTURE => {
                    eprintln!("[wgl] received overlay texture event");
                    overlay.refresh( unsafe { cmd.params.overlay_event })
                        .unwrap_or_else(|e| eprintln!("[wgl] handle error: {}", e));
                }
                _ => {}
            }
        }

        let window = unsafe { WindowFromDC(hdc) };
        wndproc.attach(window);

        if let Ok(wp) = wndproc.try_recv() {
            match wp.msg {
                WM_MOUSEMOVE => {
                    eprintln!("mm");
                }
                _ => {}
            }
        }

        let mut client_rect = Default::default();
        unsafe { GetClientRect(window, &mut client_rect) };

        let size = Dimensions {
            width: client_rect.right.abs_diff(client_rect.left),
            height: client_rect.bottom.abs_diff(client_rect.top),
        };

        if !overlay.size_matches_viewpoint(&size) {
            // if overlay is not ready to initialize then handle = 0.
            handle.send(GameWindowCommand::window_resize(&size, !overlay.ready_to_initialize()))?;
        }

        if !overlay.ready_to_initialize() {
            return Err(RenderError::OverlayHandleNotReady)
        }


        overlay.prepare_paint(gl, window, hglrc)
            .map_err(|e| RenderError::OverlayPaintNotReady(Box::new(e)))?;

        imgui.prepare_paint(gl, window, hglrc, size)
            .map_err(|e| RenderError::ImGuiNotReady(Box::new(e)))?;

        imgui.frame(&mut overlay, |ctx, render, overlay| {
            let ui = ctx.frame();
            if let Some(_kmt) = overlay.acquire_sync() {
                overlay.paint(|tid, dim|  OverlayWindow::new(&ui, tid, dim));
            }
            ui.show_metrics_window(&mut false);
            ui.show_demo_window(&mut false);
            let token = render.render(ui.render())?;
            Ok(token)
        })
    }

    fn make_swap_buffers(&self) -> FnSwapBuffersHook {
        let handle = self.ipc.clone();
        let imgui = self.imgui.clone();
        let overlay = self.overlay.clone();
        let gl = self.gl.clone();
        let ctx = self.ctx.clone();

        // let wp_r = self.wp_recv.clone();
        let wp_h = self.wp.clone();
        Box::new(move |hdc, mut next| {
            // Deal with this here instead of within the impl
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

            match WGLKernel::swapbuffers_impl(&gl.read(), handle, hdc, hglrc,overlay.write(),
                                        imgui.write(), wp_h.write()) {
                Ok(_) => {},
                Err(e) => {
                    eprintln!("[wgl] {}", e);
                }
            }
            let fp = next.fp_next();
            fp(hdc, next)
        })
    }
}