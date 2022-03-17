use imgui::TextureId;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Graphics::OpenGL::HGLRC;

use imgui_renderer_ogl::ImguiTexture;
use opengl_bindings as gl;
use opengl_bindings::Gl;
use opengl_bindings::types::{GLint, GLsizei, GLuint};

use crate::common::Dimensions;
use crate::ipc::cmd::OverlayTextureEventParams;
use crate::win32::handle::duplicate_handle;

pub(in crate::wgl) struct WGLOverlay {
    handle: HANDLE,
    window: HWND,
    context: HGLRC,
    dimensions: Dimensions,
    size: u64,
    texture: Option<GlSharedTexture>
}

struct GlSharedTexture {
    gl: Gl,
    texture: GLuint,
    memory: GLuint
}

pub(in crate::wgl) struct KeyedMutexHandle<'gl>(&'gl Gl, GLuint, u64);
impl Drop for KeyedMutexHandle<'_> {
    fn drop(&mut self) {
        unsafe {
            if self.0.ReleaseKeyedMutexWin32EXT.is_loaded() {
                self.0.ReleaseKeyedMutexWin32EXT(self.1, self.2);
            }
        }
    }
}

impl <'gl> KeyedMutexHandle<'gl> {
    pub fn new(gl: &'gl Gl, mem: GLuint, key: u64, ms: u32) -> Option<Self> {
        unsafe {
            if gl.AcquireKeyedMutexWin32EXT(mem, key, ms) == gl::TRUE {
                Some(KeyedMutexHandle(gl, mem, key))
            } else {
                None
            }
        }
    }
}

impl Drop for GlSharedTexture {
    fn drop(&mut self) {
        unsafe {
            self.gl.DeleteTextures(1, &self.texture);
            if self.gl.DeleteMemoryObjectsEXT.is_loaded() {
                self.gl.DeleteMemoryObjectsEXT(1, &self.memory)
            }
        }
    }
}

impl WGLOverlay {
    #[inline]
    pub fn ready_to_initialize(&self) -> bool {
        self.handle != HANDLE(0)
    }

    #[inline]
    const fn ready_to_paint(&self) -> bool {
        self.texture.is_some()
    }

    fn invalidate(&mut self) {
        self.texture = None;
    }

    #[inline]
    pub fn size_matches_viewpoint(&self, size: &Dimensions) -> bool {
        self.dimensions == *size
    }

    pub fn acquire_sync(&self) -> Option<KeyedMutexHandle> {
        if let Some(tex_params) = &self.texture {
            KeyedMutexHandle::new(&tex_params.gl, tex_params.memory, 0, GLuint::MAX)
        } else {
            None
        }
    }
    
    pub fn new() -> WGLOverlay {
        WGLOverlay {
            handle: HANDLE::default(),
            window: HWND::default(),
            context: HGLRC::default(),
            dimensions: Dimensions::new(0, 0),
            size: 0,
            texture: None
        }
    }

    pub fn refresh(&mut self, params: OverlayTextureEventParams) -> bool {
        let owning_pid = params.source_pid;
        let handle = HANDLE(params.handle as isize);

        // todo: extract common code
        self.handle = unsafe {
            let duped_handle = match duplicate_handle(owning_pid as u32, handle) {
                Ok(handle) => handle,
                Err(e) => {
                    eprintln!("[wgl] {:?}", e);
                    return false;
                }
            };

            // this doesn't do anything if its already null.
            self.invalidate();

            if self.ready_to_initialize() && !(CloseHandle(self.handle).as_bool()) {
                return false;
            }

            eprintln!("[wgl] duped handle {:x?}", duped_handle);
            duped_handle
        };

        self.dimensions = Dimensions {
            height: params.height,
            width: params.width
        };

        self.size = params.size;
        true
    }

    pub fn prepare_paint(&mut self, gl: &Gl, window: HWND, context: HGLRC) -> bool {
        if self.ready_to_paint() && self.window == window && self.context == context {
            return true;
        }

        self.invalidate();

        if !gl.ImportMemoryWin32HandleEXT.is_loaded()
            || !gl.TextureStorageMem2DEXT.is_loaded()
            || !gl.CreateMemoryObjectsEXT.is_loaded()
            || !gl.DeleteMemoryObjectsEXT.is_loaded()
            || !gl.AcquireKeyedMutexWin32EXT.is_loaded()
            || !gl.ReleaseKeyedMutexWin32EXT.is_loaded()
            || !gl.TextureParameteri.is_loaded()
        {
            eprintln!("[ogl] GL_EXT_memory_object_win32 not loaded or missing DSA.");
            return false;
        }

        unsafe {
            let mut texture = 0;
            gl.CreateTextures(gl::TEXTURE_2D, 1, &mut texture);
            gl.TextureParameteri(texture, gl::TEXTURE_TILING_EXT, gl::OPTIMAL_TILING_EXT as GLint);
            gl.TextureParameteri(texture, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
            gl.TextureParameteri(texture, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
            gl.TextureParameteri(texture, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as GLint);
            gl.TextureParameteri(texture, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as GLint);

            gl.TextureParameteriv(texture, gl::TEXTURE_SWIZZLE_RGBA,
                                  [gl::BLUE, gl::GREEN, gl::RED, gl::ALPHA].as_ptr() as _);

            let mut memory = 0;
            gl.CreateMemoryObjectsEXT(1, &mut memory);
            // todo: check high dpi
            gl.ImportMemoryWin32HandleEXT(memory, self.size * 2, gl::HANDLE_TYPE_D3D11_IMAGE_EXT,
                                          std::mem::transmute_copy(&self.handle));

            if gl.AcquireKeyedMutexWin32EXT(memory, 0, GLuint::MAX) == gl::TRUE {
                // todo: check gl error
                gl.TextureStorageMem2DEXT(texture, 1, gl::RGBA8, self.dimensions.width as GLsizei,
                                          self.dimensions.height as GLsizei, memory, 0);
                gl.ReleaseKeyedMutexWin32EXT(memory, 0);
            } else {
                gl.DeleteTextures(1, &texture);
                gl.DeleteMemoryObjectsEXT(1, &memory);
                eprintln!("[wgl] Could not acquire keyed mutex on memory object");
                return false;
            }

            self.texture = Some(GlSharedTexture {
                gl: gl.clone(),
                texture,
                memory
            })
        }
        true
    }

    pub fn paint<F: Sized + FnOnce(TextureId, Dimensions)>(&self, f: F) {
        if let Some(texture) = &self.texture {
            f(texture.texture.as_tex_id(), self.dimensions);
        }
    }
}

unsafe impl Send for WGLOverlay {}
unsafe impl Sync for WGLOverlay {}