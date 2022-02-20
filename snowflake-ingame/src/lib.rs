#![feature(once_cell)]

use std::error::Error;
use std::ffi::c_void;
use std::panic::catch_unwind;

use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use crate::d3d11::hook_d3d11::Direct3D11HookContext;

use crate::hook::*;
use crate::opengl::hook_opengl::OpenGLHookContext;

mod d3d11;
mod win32;
mod hook;
mod opengl;


unsafe fn main() -> Result<(), Box<dyn Error>> {
    let ctx = Direct3D11HookContext::init()?;

    ctx.new(|this, sync, flags, mut next| {
            // eprintln!("hello from hok");
            let fnext = next.fp_next();
            fnext(this, sync, flags, next)
            },
            |this, cnt, width, height, format, flags, mut next| {
            eprintln!("rz {} {}", width, height);
            let fnext = next.fp_next();
            fnext(this, cnt, width, height, format, flags, next)
        },
    )?
    .persist();

    let ctx = OpenGLHookContext::init()?;
    ctx.new(|hdc, mut next| {
        eprintln!("hello from hook!");
        let fnext = next.fp_next();
        fnext(hdc, next)
    })?.persist();
    Ok(())
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: HINSTANCE,
    call_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    // unsafe { DisableThreadLibraryCalls(module); }

    if call_reason == DLL_PROCESS_ATTACH {
        unsafe {
            AllocConsole();
        }

        std::thread::spawn(|| unsafe {
            println!(
                "{:?}",
                catch_unwind(|| {
                    match crate::main() {
                        Ok(()) => 0 as u32,
                        Err(e) => {
                            println!("Error occurred when injecting: {}", e);
                            1
                        }
                    }
                })
            );
        });
    }
    true.into()
}
