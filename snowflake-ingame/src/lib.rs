#![feature(once_cell)]

use std::cell::RefCell;
use std::error::Error;
use std::ffi::c_void;
use std::io::{BufReader, Read, Write};
use std::mem::ManuallyDrop;
use std::panic::catch_unwind;
use std::rc::Rc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use tokio::io;
use tokio::io::AsyncWriteExt;
use tokio::runtime::Runtime;
use uuid::Uuid;

use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use crate::d3d11::hook_d3d11::Direct3D11HookContext;

use crate::hook::*;
use crate::ipc::{GameWindowCommand, GameWindowCommandParams, GameWindowCommandType, GameWindowMagic, HandshakeEventParams, IpcConnection, WindowResizeEventParams};
use crate::opengl::hook_opengl::OpenGLHookContext;

mod d3d11;
mod win32;
mod hook;
mod opengl;
mod ipc;


unsafe fn main() -> Result<(), Box<dyn Error>> {

    // let (client, mut rx) = IpcConnection::new();
    let mut ipc = IpcConnection::new(Uuid::nil());
    ipc.connect(Uuid::nil())?;


    let handshake = GameWindowCommand {
            magic: GameWindowMagic::MAGIC,
            ty: GameWindowCommandType::HANDSHAKE,
            params: GameWindowCommandParams {
                handshake_event: HandshakeEventParams {
                    uuid: Uuid::nil()
                }
            }
    };

    let handle = ipc.handle().unwrap();
    // let mut ipc = Arc::new(ipc_o);
    // let ipc_h = Arc::clone(&ipc);

    let ctx = OpenGLHookContext::init()?;
    ctx.new(Box::new(move |hdc, mut next| {
        handle.send(handshake).unwrap_or_else(|_| println!("failed to send"));
        let fnext = next.fp_next();
        fnext(hdc, next)
    }))?.persist();


    ipc.listen()?;
    eprintln!("ipc stop");
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
