use std::borrow::Borrow;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::os::raw::c_char;
use imgui::DrawData;
use ouroboros::self_referencing;
use opengl_bindings::{EXTENSIONS, Gl, MAJOR_VERSION, MINOR_VERSION, NUM_EXTENSIONS, TEXTURE_2D, VERSION};
use opengl_bindings::types::{GLenum, GLint, GLuint};
use crate::backup::StateBackup;
use crate::device_objects::{FontTexture, RendererDeviceObjects, ShaderError};

#[repr(transparent)]
pub struct Renderer(RendererWrap);

impl Renderer {
    pub fn new(gl: &Gl, imgui: &mut imgui::Context) -> Result<Self, ShaderError> {
        let gl = gl.clone();

        Ok(Renderer(RendererWrap::try_new(gl, |gl| {
            RendererInner::new(&gl, imgui)
        })?))
    }

    pub fn create_device_objects(&mut self, imgui: &mut imgui::Context) -> Result<(), ShaderError> {
        self.0.with_renderer_mut(|r| r.create_device_objects(imgui))
    }

    pub fn render<'a>(&mut self, draw_data: &DrawData) -> RenderToken<'a> {
        self.0.with_renderer_mut(|r| r.render(draw_data))
    }
}

// We bother with all this because we only want to make a single clone of Gl, which is
// a relatively large struct. The device objects have to live exactly as long as
// the instance of gl.
#[self_referencing]
struct RendererWrap {
    gl: Gl,
    #[borrows(gl)]
    #[not_covariant]
    renderer: RendererInner<'this>
}

pub struct RendererInner<'gl> {
    gl: &'gl Gl,
    version: GlVersion,
    check_clip_origin: bool,
    device_objects: Option<RendererDeviceObjects<'gl>>,
    font: Option<FontTexture<'gl>>,
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct GlVersion(pub i32);

pub struct RenderToken<'a>(PhantomData<&'a ()>);

impl <'gl> RendererInner<'gl> {
    fn set_imgui_metadata(imgui: &mut imgui::Context, ver: GlVersion) {
        if ver.0 >= 320 {
            imgui.io_mut().backend_flags |= imgui::BackendFlags::RENDERER_HAS_VTX_OFFSET;
        }
        imgui.set_renderer_name(Some(format!(
            "imgui-renderer-ogl@{}",
            env!("CARGO_PKG_VERSION")
        )));
    }

    fn new(gl: &'gl Gl, imgui: &mut imgui::Context) -> Result<Self, ShaderError> {
        let version = unsafe {
            let mut maj_ver = 0;
            let mut min_ver = 0;
            gl.GetIntegerv(MAJOR_VERSION, &mut maj_ver);
            gl.GetIntegerv(MINOR_VERSION, &mut min_ver);

            if maj_ver != 0 && min_ver != 0 {
                GlVersion(maj_ver * 100 + min_ver * 10)
            } else {
                // Query GL_VERSION in desktop GL 2.x, the string will start with "<major>.<minor>"
                let ver_string = gl.GetString(VERSION);
                let ver_string = CStr::from_ptr(ver_string as *const c_char).to_string_lossy();
                let mut vers = ver_string.split(".").take(2).map(|c| c.parse::<i32>().unwrap_or(0));
                let maj_ver = vers.next().unwrap_or(0);
                let min_ver = vers.next().unwrap_or(0);
                GlVersion(maj_ver * 100 + min_ver * 10)
            }
        };

        // determine whether or not to test clip_origin on backup
        let mut check_clip_origin = version.0 >= 450;
        unsafe {
            let mut num_ext = 0;
            gl.GetIntegerv(NUM_EXTENSIONS, &mut num_ext);

            for i in 0..num_ext {
                let ext_str = gl.GetStringi(EXTENSIONS, i as GLuint);
                if ext_str.is_null() {
                    continue;
                }

                let ext_str = CStr::from_ptr(ext_str as *const c_char).to_string_lossy();
                if ext_str == "GL_ARB_clip_control" {
                    check_clip_origin = true;
                }
            }
        }


        let mut renderer = RendererInner {
            gl,
            version,
            check_clip_origin,
            device_objects: None,
            font: None,
        };

        renderer.create_device_objects(imgui)?;
        Self::set_imgui_metadata(imgui, version);
        Ok(renderer)
    }

    fn create_device_objects(&mut self, imgui: &mut imgui::Context) -> Result<(), ShaderError> {
        let device_objects = RendererDeviceObjects::new(&self.gl, self.version)?;
        let mut imgui_fonts = imgui.fonts();
        let fonts = FontTexture::new(&mut imgui_fonts, &self.gl);
        imgui_fonts.tex_id = fonts.tex_id();
        self.font = Some(fonts);
        self.device_objects = Some(device_objects);
        Ok(())
    }

    fn render<'a>(&mut self, draw_data: &DrawData) -> RenderToken<'a> {
        let state_backup = StateBackup::new(self.gl, self.version);
        let mut x = 0;
        unsafe {
            self.gl.GetIntegerv(TEXTURE_2D, &mut x);
        }
        RenderToken(PhantomData)
    }
}