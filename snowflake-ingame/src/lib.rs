#![feature(once_cell)]

use std::error::Error;
use std::ffi::c_void;
use std::io::{Read, Write};
use std::panic::catch_unwind;

use tokio::io::AsyncWriteExt;
use uuid::Uuid;
use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

use crate::d3d11::hook_d3d11::Direct3D11HookContext;
use crate::hook::*;
use crate::ipc::cmd::GameWindowCommand;
use crate::ipc::{IpcConnection, IpcConnectionBuilder};
use crate::opengl::hook_opengl::OpenGLHookContext;

mod d3d11;
mod win32;
mod hook;
mod opengl;
mod ipc;


unsafe fn main() -> Result<(), Box<dyn Error>> {
    let mut ipc = IpcConnectionBuilder::new(Uuid::nil());
    let mut ipc = ipc.connect()?;

    let handle = ipc.handle();

    let ctx = OpenGLHookContext::init()?;

    let handshake = GameWindowCommand::handshake(&Uuid::nil());

    ctx.new(Box::new(move |hdc, mut next| {
        handle.send(handshake).unwrap_or_else(|_| println!("failed to send"));
        let fnext = next.fp_next();
        fnext(hdc, next)
    }))?.persist();


    ipc.listen()?;
    // eprintln!("ipc stop");
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
            println!("over.");
        });

    }
    true.into()
}
