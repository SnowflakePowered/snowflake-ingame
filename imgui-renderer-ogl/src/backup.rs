use opengl_bindings::types::{GLboolean, GLenum, GLint, GLuint};
use opengl_bindings::*;

pub struct StateBackup<'gl> {
    gl: &'gl Gl,
    last_active_texture: GLint,
    last_program: GLint,
    last_texture: GLint,
    last_sampler: GLint,
    last_array_buffer: GLint,
    last_vertex_array: GLint,
    last_polygon_mode: [GLint; 2],
    last_viewport: [GLint; 4],
    last_scissor_box: [GLint; 4],
    last_blend_src_rgb: GLint,
    last_blend_dst_rgb: GLint,
    last_blend_equation_rgb: GLint,
    last_blend_equation_alpha: GLint,
    last_enable_blend: GLboolean,
    last_enable_cull_face: GLboolean,
    last_enable_depth_test: GLboolean,
    last_enable_stencil_test: GLboolean,
    last_enable_scissor_test: GLboolean,
    last_enable_primitive_restart: GLboolean,
    pub last_blend_src_alpha: GLint,
    pub last_blend_dst_alpha: GLint,
}

macro_rules! backup_gl_int_state {
    (let mut $var:ident as $glenum:expr; $gl:ident) => {
        let mut $var = 0;
        $gl.GetIntegerv($glenum, &mut $var)
    };
    (let mut $var:ident as $glenum:expr; if $if:expr; $gl:ident) => {
        let mut $var = 0;
        if $if {
            $gl.GetIntegerv($glenum, &mut $var);
        }
    };
    (let mut $var:ident[$num:literal] as $glenum:expr; $gl:ident) => {
        let mut $var = [0; $num];
        $gl.GetIntegerv($glenum, $var.as_mut_ptr())
    };
    (let mut $var:ident[$num:literal] as $glenum:expr; if $if:expr; $gl:ident) => {
        let mut $var = [0; $num];
        if $if {
            $gl.GetIntegerv($glenum, $var.as_mut_ptr())
        }
    };
    (let $var:ident as $glenum:expr; $gl:ident) => {
        let $var = $gl.IsEnabled($glenum);
    };
    (let $var:ident as $glenum:expr; if $if:expr; $gl:ident) => {
        let $var = if $if {
            $gl.IsEnabled($glenum)
        } else {
            opengl_bindings::FALSE
        };
    };
}

impl<'gl> StateBackup<'gl> {
    pub fn new(gl: &'gl Gl) -> Self {
        unsafe {
            backup_gl_int_state!(let mut last_active_texture as ACTIVE_TEXTURE; gl);
            gl.ActiveTexture(TEXTURE0);

            backup_gl_int_state!(let mut last_program as CURRENT_PROGRAM; gl);
            backup_gl_int_state!(let mut last_texture as TEXTURE_BINDING_2D; gl);
            backup_gl_int_state!(let mut last_sampler as SAMPLER_BINDING; if gl.BindSampler.is_loaded(); gl);
            backup_gl_int_state!(let mut last_array_buffer as ARRAY_BUFFER_BINDING; gl);

            backup_gl_int_state!(let mut last_vertex_array as VERTEX_ARRAY;
                if gl.BindVertexArray.is_loaded(); gl);
            backup_gl_int_state!(let mut last_polygon_mode[2] as POLYGON_MODE;
                if gl.PolygonMode.is_loaded(); gl);

            backup_gl_int_state!(let mut last_viewport[4] as VIEWPORT; gl);
            backup_gl_int_state!(let mut last_scissor_box[4] as SCISSOR_BOX; gl);

            backup_gl_int_state!(let mut last_blend_src_rgb as BLEND_SRC_RGB; gl);
            backup_gl_int_state!(let mut last_blend_dst_rgb as BLEND_DST_RGB; gl);

            backup_gl_int_state!(let mut last_blend_src_alpha as BLEND_SRC_ALPHA; gl);
            backup_gl_int_state!(let mut last_blend_dst_alpha as BLEND_DST_ALPHA; gl);

            backup_gl_int_state!(let mut last_blend_equation_rgb as BLEND_EQUATION_RGB; gl);
            backup_gl_int_state!(let mut last_blend_equation_alpha as BLEND_EQUATION_ALPHA; gl);

            backup_gl_int_state!(let last_enable_blend as BLEND; gl);
            backup_gl_int_state!(let last_enable_cull_face as CULL_FACE; gl);
            backup_gl_int_state!(let last_enable_depth_test as DEPTH_TEST; gl);
            backup_gl_int_state!(let last_enable_stencil_test as STENCIL_TEST; gl);
            backup_gl_int_state!(let last_enable_scissor_test as SCISSOR_TEST; gl);

            backup_gl_int_state!(let last_enable_primitive_restart as PRIMITIVE_RESTART;
                if gl.PrimitiveRestartIndex.is_loaded(); gl);

            StateBackup {
                gl,
                last_active_texture,
                last_program,
                last_texture,
                last_sampler,
                last_array_buffer,
                last_vertex_array,
                last_polygon_mode,
                last_viewport,
                last_scissor_box,
                last_blend_src_rgb,
                last_blend_dst_rgb,
                last_blend_src_alpha,
                last_blend_dst_alpha,
                last_blend_equation_rgb,
                last_blend_equation_alpha,
                last_enable_blend,
                last_enable_cull_face,
                last_enable_depth_test,
                last_enable_stencil_test,
                last_enable_scissor_test,
                last_enable_primitive_restart,
            }
        }
    }
}

impl Drop for StateBackup<'_> {
    fn drop(&mut self) {
        let gl = self.gl;
        unsafe {
            gl.UseProgram(self.last_program as GLuint);
            gl.BindTexture(TEXTURE_2D, self.last_texture as GLuint);
            if gl.BindSampler.is_loaded() {
                gl.BindSampler(0, self.last_sampler as GLuint);
            }
            gl.ActiveTexture(self.last_active_texture as GLuint);

            if gl.BindVertexArray.is_loaded() {
                gl.BindVertexArray(self.last_vertex_array as GLuint);
            }

            gl.BindBuffer(ARRAY_BUFFER, self.last_array_buffer as GLuint);
            gl.BlendEquationSeparate(
                self.last_blend_equation_rgb as GLenum,
                self.last_blend_equation_alpha as GLenum,
            );
            gl.BlendFuncSeparate(
                self.last_blend_src_rgb as GLenum,
                self.last_blend_dst_rgb as GLenum,
                self.last_blend_src_alpha as GLenum,
                self.last_blend_dst_alpha as GLenum,
            );

            if self.last_enable_blend == TRUE {
                gl.Enable(BLEND)
            } else {
                gl.Disable(BLEND)
            }

            if self.last_enable_cull_face == TRUE {
                gl.Enable(CULL_FACE)
            } else {
                gl.Disable(CULL_FACE)
            }

            if self.last_enable_depth_test == TRUE {
                gl.Enable(DEPTH_TEST)
            } else {
                gl.Disable(DEPTH_TEST)
            }

            if self.last_enable_stencil_test == TRUE {
                gl.Enable(STENCIL_TEST)
            } else {
                gl.Disable(STENCIL_TEST)
            }

            if self.last_enable_scissor_test == TRUE {
                gl.Enable(SCISSOR_TEST)
            } else {
                gl.Disable(SCISSOR_TEST)
            }

            if gl.PrimitiveRestartIndex.is_loaded() {
                if self.last_enable_primitive_restart == TRUE {
                    gl.Enable(PRIMITIVE_RESTART)
                } else {
                    gl.Disable(PRIMITIVE_RESTART)
                }
            }

            if gl.PolygonMode.is_loaded() {
                gl.PolygonMode(FRONT_AND_BACK, self.last_polygon_mode[0] as GLenum);
            }

            gl.Viewport(
                self.last_viewport[0],
                self.last_viewport[1],
                self.last_viewport[2],
                self.last_viewport[3],
            );
            gl.Scissor(
                self.last_scissor_box[0],
                self.last_scissor_box[1],
                self.last_scissor_box[2],
                self.last_scissor_box[3],
            );
        }
    }
}
