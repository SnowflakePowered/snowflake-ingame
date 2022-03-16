use std::ffi::CStr;
use std::marker::PhantomData;
use std::mem;
use std::os::raw::c_char;

use field_offset::offset_of;
use imgui::internal::RawWrapper;
use imgui::{DrawCmd, DrawData, DrawIdx, DrawVert};
use ouroboros::self_referencing;

use opengl_bindings::types::{GLenum, GLint, GLsizei, GLuint};
use opengl_bindings::{
    Gl, ARRAY_BUFFER, BLEND, CLIP_ORIGIN, CULL_FACE, DEPTH_TEST, ELEMENT_ARRAY_BUFFER, FILL,
    FRONT_AND_BACK, FUNC_ADD, MAJOR_VERSION, MINOR_VERSION, ONE, ONE_MINUS_SRC_ALPHA,
    PRIMITIVE_RESTART, SCISSOR_TEST, SRC_ALPHA, STENCIL_TEST, STREAM_DRAW, TEXTURE_2D, TRIANGLES,
    UNSIGNED_INT, UNSIGNED_SHORT, UPPER_LEFT, VERSION,
};

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
    renderer: RendererInner<'this>,
}

pub struct RendererInner<'gl> {
    gl: &'gl Gl,
    version: GlVersion,
    // check_clip_origin: bool,
    device_objects: Option<RendererDeviceObjects<'gl>>,
    font: Option<FontTexture<'gl>>,
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone)]
pub struct GlVersion(pub i32);

pub struct RenderToken<'a>(PhantomData<&'a ()>);

impl<'gl> RendererInner<'gl> {
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
                let mut vers = ver_string
                    .split(".")
                    .take(2)
                    .map(|c| c.parse::<i32>().unwrap_or(0));
                let maj_ver = vers.next().unwrap_or(0);
                let min_ver = vers.next().unwrap_or(0);
                GlVersion(maj_ver * 100 + min_ver * 10)
            }
        };

        // don't need because we can just rely on the loader

        // determine whether or not to test clip_origin on backup
        // let mut check_clip_origin = version.0 >= 450;
        // unsafe {
        //     let mut num_ext = 0;
        //     gl.GetIntegerv(NUM_EXTENSIONS, &mut num_ext);
        //
        //     for i in 0..num_ext {
        //         let ext_str = gl.GetStringi(EXTENSIONS, i as GLuint);
        //         if ext_str.is_null() {
        //             continue;
        //         }
        //
        //         let ext_str = CStr::from_ptr(ext_str as *const c_char).to_string_lossy();
        //         if ext_str == "GL_ARB_clip_control" {
        //             check_clip_origin = true;
        //         }
        //     }
        // }

        let mut renderer = RendererInner {
            gl,
            version,
            // check_clip_origin,
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
        // Avoid rendering when minimized
        let fb_width = draw_data.display_size[0] * draw_data.framebuffer_scale[0];
        let fb_height = draw_data.display_size[1] * draw_data.framebuffer_scale[1];

        if fb_height <= 0.0 || fb_width <= 0.0 || draw_data.draw_lists_count() == 0 {
            return RenderToken(PhantomData);
        }

        unsafe {
            let state = StateBackup::new(self.gl);
            let mut vertex_array = 0;

            if self.gl.GenVertexArrays.is_loaded() {
                self.gl.GenVertexArrays(1, &mut vertex_array);
            }

            self.setup_render_state(draw_data, fb_width, fb_height, vertex_array);
            self.render_cmd_lists(draw_data, fb_width, fb_height, vertex_array);

            if self.gl.DeleteVertexArrays.is_loaded() {
                self.gl.DeleteVertexArrays(1, &vertex_array);
            }
            drop(state);
        }
        RenderToken(PhantomData)
    }

    // Safety: vertex_array is not 0 if gl.BindVertexArray is loaded
    unsafe fn setup_render_state(
        &self,
        draw_data: &DrawData,
        fb_width: f32,
        fb_height: f32,
        vertex_array: GLuint,
    ) {
        let gl = self.gl;

        gl.Enable(BLEND);
        gl.BlendEquation(FUNC_ADD);
        gl.BlendFuncSeparate(SRC_ALPHA, ONE_MINUS_SRC_ALPHA, ONE, ONE_MINUS_SRC_ALPHA);
        gl.Disable(CULL_FACE);
        gl.Disable(DEPTH_TEST);
        gl.Disable(STENCIL_TEST);
        gl.Enable(SCISSOR_TEST);

        if gl.PrimitiveRestartIndex.is_loaded() {
            gl.Disable(PRIMITIVE_RESTART)
        }

        if gl.PolygonMode.is_loaded() {
            gl.PolygonMode(FRONT_AND_BACK, FILL);
        }

        let mut clip_origin_lower_left = true;
        if gl.ClipControl.is_loaded() {
            let mut current_clip_origin = 0;
            gl.GetIntegerv(CLIP_ORIGIN, &mut current_clip_origin);
            if current_clip_origin == UPPER_LEFT as GLint {
                clip_origin_lower_left = false;
            }
        }

        // setup viewport
        gl.Viewport(0, 0, fb_width as GLsizei, fb_height as GLsizei);

        let l = draw_data.display_pos[0];
        let r = draw_data.display_pos[0] + draw_data.display_size[0];
        let mut t = draw_data.display_pos[1];
        let mut b = draw_data.display_pos[1] + draw_data.display_size[1];

        if !clip_origin_lower_left {
            (t, b) = (b, t) // Swap top and bottom if origin is upper left
        }

        let mvp = [
            [2.0 / (r - l), 0.0, 0.0, 0.0],
            [0.0, 2.0 / (t - b), 0.0, 0.0],
            [0.0, 0.0, 0.5, 0.0],
            [(r + l) / (l - r), (t + b) / (b - t), 0.5, 1.0],
        ];

        if let Some(device_objects) = &self.device_objects {
            gl.UseProgram(device_objects.shader.handle);
            gl.Uniform1i(device_objects.shader.attrib_loc_tex, 0);
            gl.UniformMatrix4fv(
                device_objects.shader.attrib_loc_proj_mtx,
                1,
                opengl_bindings::FALSE,
                mvp.as_ptr() as _,
            );

            if gl.BindSampler.is_loaded() {
                gl.BindSampler(0, 0);
            }

            if gl.BindVertexArray.is_loaded() {
                gl.BindVertexArray(vertex_array)
            }

            // Bind vertex/index buffers and setup attributes for ImDrawVert
            gl.BindBuffer(ARRAY_BUFFER, device_objects.vertex_buffer_obj);
            gl.BindBuffer(ELEMENT_ARRAY_BUFFER, device_objects.elements_buffer_obj);

            gl.EnableVertexAttribArray(device_objects.shader.attrib_loc_vtx_pos);
            gl.EnableVertexAttribArray(device_objects.shader.attrib_loc_vtx_uv);
            gl.EnableVertexAttribArray(device_objects.shader.attrib_loc_vtx_color);

            gl.VertexAttribPointer(
                device_objects.shader.attrib_loc_vtx_pos,
                2,
                opengl_bindings::FLOAT,
                opengl_bindings::FALSE,
                mem::size_of::<DrawVert>() as _,
                offset_of!(DrawVert => pos).get_byte_offset() as _,
            );

            gl.VertexAttribPointer(
                device_objects.shader.attrib_loc_vtx_uv,
                2,
                opengl_bindings::FLOAT,
                opengl_bindings::FALSE,
                mem::size_of::<DrawVert>() as _,
                offset_of!(DrawVert => uv).get_byte_offset() as _,
            );

            gl.VertexAttribPointer(
                device_objects.shader.attrib_loc_vtx_color,
                4,
                opengl_bindings::UNSIGNED_BYTE,
                opengl_bindings::TRUE,
                mem::size_of::<DrawVert>() as _,
                offset_of!(DrawVert => col).get_byte_offset() as _,
            );
        }
    }

    unsafe fn render_cmd_lists(
        &self,
        draw_data: &DrawData,
        fb_width: f32,
        fb_height: f32,
        vertex_array: GLuint,
    ) {
        if let Some(device_objects) = &self.device_objects {
            let clip_off = draw_data.display_pos;
            let clip_scale = draw_data.framebuffer_scale;

            for draw_list in draw_data.draw_lists() {
                let vtx_buffer = draw_list.vtx_buffer();
                let idx_buffer = draw_list.idx_buffer();

                self.gl
                    .BindBuffer(ARRAY_BUFFER, device_objects.vertex_buffer_obj);
                self.gl.BufferData(
                    ARRAY_BUFFER,
                    (vtx_buffer.len() * mem::size_of::<DrawVert>()) as _,
                    vtx_buffer.as_ptr() as _,
                    STREAM_DRAW,
                );

                self.gl
                    .BindBuffer(ELEMENT_ARRAY_BUFFER, device_objects.elements_buffer_obj);
                self.gl.BufferData(
                    ELEMENT_ARRAY_BUFFER,
                    (idx_buffer.len() * mem::size_of::<DrawIdx>()) as _,
                    idx_buffer.as_ptr() as _,
                    STREAM_DRAW,
                );

                for cmd in draw_list.commands() {
                    match cmd {
                        DrawCmd::RawCallback { callback, raw_cmd } => {
                            callback(draw_list.raw(), raw_cmd)
                        }
                        DrawCmd::ResetRenderState => {
                            self.setup_render_state(draw_data, fb_width, fb_height, vertex_array)
                        }
                        DrawCmd::Elements { count, cmd_params } => {
                            self.gl
                                .BindTexture(TEXTURE_2D, cmd_params.texture_id.id() as _);
                            // X Y Z W
                            let [clip_x, clip_y, clip_z, clip_w] = cmd_params.clip_rect;
                            let [off_x, off_y] = clip_off;
                            let [scale_x, scale_y] = clip_scale;

                            let clip_min = ((clip_x - off_x) * scale_x, (clip_y - off_y) * scale_y);
                            let clip_max = ((clip_z - off_x) * scale_x, (clip_w - off_y) * scale_y);

                            if clip_max.0 <= clip_min.0 || clip_max.1 <= clip_min.1 {
                                continue;
                            }

                            self.gl.Scissor(
                                clip_min.0 as GLint,
                                (fb_height - clip_max.1) as GLint,
                                (clip_max.0 - clip_min.0) as GLsizei,
                                (clip_max.1 - clip_min.1) as GLsizei,
                            );

                            self.gl.DrawElements(
                                TRIANGLES,
                                count as _,
                                IDX_SIZE,
                                (cmd_params.idx_offset * mem::size_of::<DrawIdx>()) as _,
                            )
                        }
                    }
                }
            }
        }
    }
}

const IDX_SIZE: GLenum = idx_size();
const fn idx_size() -> GLenum {
    if mem::size_of::<DrawIdx>() == 2 {
        UNSIGNED_SHORT
    } else {
        UNSIGNED_INT
    }
}
