use gl_generator::{Api, Fallbacks, Profile, Registry, StructGenerator};
use std::env;
use std::fs::File;
use std::path::Path;

fn main() {
    let dest = env::var("OUT_DIR").unwrap();
    let mut file = File::create(&Path::new(&dest).join("gl_bindings.rs")).unwrap();

    Registry::new(
        Api::Gl,
        (4, 6),
        Profile::Core,
        Fallbacks::All,
        [
            "GL_EXT_memory_object",
            "GL_EXT_semaphore",
            "GL_EXT_memory_object_win32",
            "GL_EXT_semaphore_win32",
            "GL_EXT_win32_keyed_mutex",
            "GL_EXT_memory_object_fd",
            "GL_EXT_semaphore_fd",
        ],
    )
    .write_bindings(StructGenerator, &mut file)
    .unwrap();
}
