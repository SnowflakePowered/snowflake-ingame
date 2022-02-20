use gl_generator::{Registry, Api, Profile, Fallbacks, GlobalGenerator};
use std::env;
use std::fs::File;
use std::path::Path;

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("gl_bindings.rs")).unwrap();

    Registry::new(Api::Gl, (4, 6), Profile::Core, Fallbacks::All, [
        "GL_EXT_memory_object",
        "GL_EXT_semaphore",
        "GL_EXT_memory_object_win32",
        "GL_EXT_semaphore_win32",
        "GL_EXT_win32_keyed_mutex"
    ]).write_bindings( GlobalGenerator, &mut file).unwrap();
}