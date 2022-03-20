use crate::renderer::GlVersion;
use crate::{ImguiTexture, RenderError};
use imgui::TextureId;
use opengl_bindings::types::{GLchar, GLenum, GLint, GLsizei, GLuint};
use opengl_bindings::{
    Gl, ARRAY_BUFFER, ARRAY_BUFFER_BINDING, COMPILE_STATUS, FRAGMENT_SHADER, INFO_LOG_LENGTH,
    LINEAR, LINK_STATUS, RGBA, TEXTURE_2D, TEXTURE_BINDING_2D, TEXTURE_MAG_FILTER,
    TEXTURE_MIN_FILTER, UNPACK_ROW_LENGTH, UNSIGNED_BYTE, VERTEX_ARRAY_BINDING, VERTEX_SHADER,
};
use std::os::raw::c_void;

const FRAGMENT_120: &'static [u8] = include_bytes!("shaders/fragment_shader.120.glsl");
const FRAGMENT_130: &'static [u8] = include_bytes!("shaders/fragment_shader.130.glsl");
const FRAGMENT_300: &'static [u8] = include_bytes!("shaders/fragment_shader.300.glsl");

const VERTEX_120: &'static [u8] = include_bytes!("shaders/vertex_shader.120.glsl");
const VERTEX_130: &'static [u8] = include_bytes!("shaders/vertex_shader.130.glsl");
const VERTEX_300: &'static [u8] = include_bytes!("shaders/vertex_shader.300.glsl");

struct Shader {
    source: &'static [u8],
    version: &'static [u8],
    ty: GLenum,
}

struct CompiledShader<'a> {
    gl: &'a Gl,
    shader: GLuint,
}

impl Drop for CompiledShader<'_> {
    fn drop(&mut self) {
        unsafe {
            self.gl.DeleteShader(self.shader);
        }
    }
}

impl Shader {
    const fn glsl_version(version: GlVersion) -> &'static [u8] {
        match version {
            GlVersion(100) => b"#version 100\n", // ES2.0
            GlVersion(110) => b"#version 110\n",
            GlVersion(120) => b"#version 120\n",
            GlVersion(130) => b"#version 130\n",
            GlVersion(140) => b"#version 140\n",
            GlVersion(150) => b"#version 150\n",
            GlVersion(300) => b"#version 300 es\n", // ES2.0
            GlVersion(330) => b"#version 330 core\n",
            GlVersion(400) => b"#version 400 core\n",
            GlVersion(410) => b"#version 410 core\n",
            GlVersion(420) => b"#version 410 core\n",
            GlVersion(430..=499) => b"#version 430 core\n",
            #[cfg(target_os = "macos")]
            _ => b"#version 150\n", // default to 150 on apple

            _ => b"#version 130\n", // default to 130
        }
    }

    const fn glsl_vertex(version: GlVersion) -> &'static [u8] {
        if version.0 < 130 {
            return VERTEX_120;
        }
        if version.0 >= 300 {
            return VERTEX_300;
        }
        return VERTEX_130;
    }

    const fn glsl_fragment(version: GlVersion) -> &'static [u8] {
        if version.0 < 130 {
            return FRAGMENT_120;
        }
        if version.0 >= 300 {
            return FRAGMENT_300;
        }
        return FRAGMENT_130;
    }

    pub const fn fragment_shader(version: GlVersion) -> Shader {
        Shader {
            version: Shader::glsl_version(version),
            source: Shader::glsl_fragment(version),
            ty: FRAGMENT_SHADER,
        }
    }

    pub const fn vertex_shader(version: GlVersion) -> Shader {
        Shader {
            version: Shader::glsl_version(version),
            source: Shader::glsl_vertex(version),
            ty: VERTEX_SHADER,
        }
    }

    fn check_shader(&self, gl: &Gl, handle: GLuint) -> Result<(), RenderError> {
        let mut status = 0;
        let mut log_length = 0;
        unsafe {
            let version = std::str::from_utf8_unchecked(self.version);
            gl.GetShaderiv(handle, COMPILE_STATUS, &mut status);
            gl.GetShaderiv(handle, INFO_LOG_LENGTH, &mut log_length);
            if status == opengl_bindings::FALSE as GLint {
                return Err(RenderError::CompileError(self.ty, Box::new(version)));
            }
        }
        Ok(())
    }

    pub fn compile(self, gl: &Gl) -> Result<CompiledShader, RenderError> {
        let source = [
            self.version.as_ptr() as *const GLchar,
            self.source.as_ptr() as *const GLchar,
        ];
        let lengths = [self.version.len() as GLint, self.source.len() as GLint];
        unsafe {
            let handle = gl.CreateShader(self.ty);
            gl.ShaderSource(handle, 2, source.as_ptr(), lengths.as_ptr());
            gl.CompileShader(handle);
            self.check_shader(gl, handle)?;
            Ok(CompiledShader { gl, shader: handle })
        }
    }
}

pub(crate) struct Program<'gl> {
    pub handle: GLuint,
    pub attrib_loc_tex: GLint,
    pub attrib_loc_proj_mtx: GLint,
    pub attrib_loc_vtx_pos: GLuint,
    pub attrib_loc_vtx_uv: GLuint,
    pub attrib_loc_vtx_color: GLuint,
    gl: &'gl Gl,
}

impl<'gl> Program<'gl> {
    fn check_program(gl: &Gl, handle: GLuint) -> Result<(), RenderError> {
        let mut status = 0;
        let mut log_length = 0;
        unsafe {
            gl.GetProgramiv(handle, LINK_STATUS, &mut status);
            gl.GetProgramiv(handle, INFO_LOG_LENGTH, &mut log_length);
            if status == opengl_bindings::FALSE as GLint {
                return Err(RenderError::LinkError);
            }
        }
        Ok(())
    }

    pub fn new(gl: &'gl Gl, version: GlVersion) -> Result<Program<'gl>, RenderError> {
        let vertex_shader = Shader::vertex_shader(version).compile(gl)?;
        let fragment_shader = Shader::fragment_shader(version).compile(gl)?;

        unsafe {
            let handle = gl.CreateProgram();
            gl.AttachShader(handle, vertex_shader.shader);
            gl.AttachShader(handle, fragment_shader.shader);
            gl.LinkProgram(handle);

            Program::check_program(gl, handle)?;

            gl.DetachShader(handle, vertex_shader.shader);
            gl.DetachShader(handle, fragment_shader.shader);

            let attrib_loc_tex = gl.GetUniformLocation(handle, b"Texture\0".as_ptr() as _);
            let attrib_loc_proj_mtx = gl.GetUniformLocation(handle, b"ProjMtx\0".as_ptr() as _);
            let attrib_loc_vtx_pos =
                gl.GetAttribLocation(handle, b"Position\0".as_ptr() as _) as GLuint;
            let attrib_loc_vtx_uv = gl.GetAttribLocation(handle, b"UV\0".as_ptr() as _) as GLuint;
            let attrib_loc_vtx_color =
                gl.GetAttribLocation(handle, b"Color\0".as_ptr() as _) as GLuint;

            Ok(Program {
                handle,
                attrib_loc_tex,
                attrib_loc_proj_mtx,
                attrib_loc_vtx_pos,
                attrib_loc_vtx_uv,
                attrib_loc_vtx_color,
                gl,
            })
        }
    }
}

impl Drop for Program<'_> {
    fn drop(&mut self) {
        unsafe {
            self.gl.DeleteProgram(self.handle);
        }
    }
}

pub(crate) struct RendererDeviceObjects<'gl> {
    pub shader: Program<'gl>,
    pub vertex_buffer_obj: GLuint,
    pub elements_buffer_obj: GLuint,
    gl: &'gl Gl,
}

impl<'gl> RendererDeviceObjects<'gl> {
    pub fn new(gl: &'gl Gl, version: GlVersion) -> Result<RendererDeviceObjects<'gl>, RenderError> {
        // backup state
        let mut last_texture = 0;
        let mut last_array_buffer = 0;
        let mut last_vertex_array = 0;

        unsafe {
            gl.GetIntegerv(TEXTURE_BINDING_2D, &mut last_texture);
            gl.GetIntegerv(ARRAY_BUFFER_BINDING, &mut last_array_buffer);
            gl.GetIntegerv(VERTEX_ARRAY_BINDING, &mut last_vertex_array);
        }

        let shader = Program::new(gl, version)?;

        let mut vbo = 0;
        let mut elements = 0;

        unsafe {
            gl.GenBuffers(1, &mut vbo);
            gl.GenBuffers(1, &mut elements);
        }

        // Restore state
        unsafe {
            gl.BindTexture(TEXTURE_2D, last_texture as GLuint);
            gl.BindBuffer(ARRAY_BUFFER, last_array_buffer as GLuint);
            gl.BindVertexArray(last_vertex_array as GLuint);
        }

        Ok(RendererDeviceObjects {
            gl,
            shader,
            vertex_buffer_obj: vbo,
            elements_buffer_obj: elements,
        })
    }
}

impl Drop for RendererDeviceObjects<'_> {
    fn drop(&mut self) {
        unsafe {
            self.gl.DeleteBuffers(1, &self.vertex_buffer_obj);
            self.gl.DeleteBuffers(1, &self.elements_buffer_obj)
        }
    }
}

pub struct FontTexture<'gl> {
    handle: GLuint,
    gl: &'gl Gl,
}

impl<'gl> FontTexture<'gl> {
    pub fn new(fonts: &mut imgui::FontAtlasRefMut<'_>, gl: &'gl Gl) -> FontTexture<'gl> {
        let mut last_texture = 0;
        let mut font_texture = 0;
        let font_tex_data = fonts.build_rgba32_texture();

        unsafe {
            // backup state
            // todo: don't know if this needs to be done under RDO::new
            gl.GetIntegerv(TEXTURE_BINDING_2D, &mut last_texture);

            gl.GenTextures(1, &mut font_texture);
            gl.BindTexture(TEXTURE_2D, font_texture);
            gl.TexParameteri(TEXTURE_2D, TEXTURE_MIN_FILTER, LINEAR as GLint);
            gl.TexParameteri(TEXTURE_2D, TEXTURE_MAG_FILTER, LINEAR as GLint);
            gl.PixelStorei(UNPACK_ROW_LENGTH, 0);
            gl.TexImage2D(
                TEXTURE_2D,
                0,
                RGBA as GLint,
                font_tex_data.width as GLsizei,
                font_tex_data.height as GLsizei,
                0,
                RGBA,
                UNSIGNED_BYTE,
                font_tex_data.data.as_ptr() as *const c_void,
            );

            gl.BindTexture(TEXTURE_2D, last_texture as GLuint);
        }

        FontTexture {
            handle: font_texture,
            gl,
        }
    }

    pub fn tex_id(&self) -> TextureId {
        self.handle.as_tex_id()
    }
}

impl Drop for FontTexture<'_> {
    fn drop(&mut self) {
        unsafe { self.gl.DeleteTextures(1, &self.handle) }
    }
}
