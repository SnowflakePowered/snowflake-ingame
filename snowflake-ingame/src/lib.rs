#![feature(once_cell)]

use std::error::Error;
use std::ffi::c_void;
use std::io::{BufReader, Read, Write};
use std::panic::catch_unwind;
use std::thread;
use tokio::io::AsyncWriteExt;
use tokio::runtime::Runtime;
use uuid::Uuid;

use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use crate::d3d11::hook_d3d11::Direct3D11HookContext;

use crate::hook::*;
use crate::ipc::{GameWindowCommand, GameWindowCommandParams, GameWindowCommandType, GameWindowMagic, HandshakeEventParams};
use crate::opengl::hook_opengl::OpenGLHookContext;

mod d3d11;
mod win32;
mod hook;
mod opengl;
mod ipc;


unsafe fn main() -> Result<(), Box<dyn Error>> {
    let rt  = Runtime::new()?;
    rt.block_on(async {
        // let ctx = Direct3D11HookContext::init()?;
        //
        //
        // ctx.new(|this, sync, flags, mut next| {
        //         // eprintln!("hello from hok");
        //         let fnext = next.fp_next();
        //         fnext(this, sync, flags, next)
        //         },
        //         |this, cnt, width, height, format, flags, mut next| {
        //         eprintln!("rz {} {}", width, height);
        //         let fnext = next.fp_next();
        //         fnext(this, cnt, width, height, format, flags, next)
        //     },
        // )?
        // .persist();

        let handshake = GameWindowCommand {
            magic: GameWindowMagic::MAGIC,
            ty: GameWindowCommandType::HANDSHAKE,
            params: GameWindowCommandParams {
                handshake_event: HandshakeEventParams {
                    uuid: Uuid::nil()
                }
            }
        };

        let mut client = ipc::connect(Uuid::nil()).await?;

        let mut buf: &[u8] = (&handshake).into();
        client.write_buf(&mut buf);

        let ctx = OpenGLHookContext::init()?;
        ctx.new(Box::new(move |hdc, mut next| {
            // eprintln!("hello from hook!");
            let fnext = next.fp_next();
            fnext(hdc, next)
        }))?.persist();

        Ok(())
    })
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
