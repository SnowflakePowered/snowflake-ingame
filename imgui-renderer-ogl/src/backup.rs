use opengl_bindings::{ACTIVE_TEXTURE, Gl, POLYGON_MODE, TEXTURE0};
use opengl_bindings::types::{GLint, GLuint};
use crate::renderer::GlVersion;

pub struct StateBackup<'gl> {
    gl: &'gl Gl,
    version: GlVersion,
    last_active_texture: GLint
}

macro_rules! backup_gl_state {
    
}

impl <'gl> StateBackup<'gl> {
    pub fn new(gl: &'gl Gl, version: GlVersion) -> Self {
        unsafe {
            // active texture
            let mut last_active_texture = 0;
            gl.GetIntegerv(ACTIVE_TEXTURE, &mut last_active_texture);
            gl.ActiveTexture(TEXTURE0);

            let mut last_polygon_mode = [0;2];
            gl.GetIntegerv(POLYGON_MODE, last_polygon_mode.as_mut_ptr());
            StateBackup {
                gl,
                version,
                last_active_texture
            }
        }

    }
}

impl Drop for StateBackup<'_> {
    fn drop(&mut self) {
        todo!()
    }
}