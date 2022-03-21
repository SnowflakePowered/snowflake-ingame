#![feature(once_cell)]
#![feature(type_alias_impl_trait)]
#![feature(associated_type_defaults)]

use std::error::Error;
use std::ffi::c_void;
use std::panic::catch_unwind;
use std::sync::Arc;
use imgui::Context;
use parking_lot::RwLock;

use uuid::Uuid;
use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

use crate::d3d11::Direct3D11Kernel;
use crate::hook::*;
use crate::ipc::IpcConnectionBuilder;
use crate::kernel::common::{FrameKernel, KernelContext};
use crate::wgl::WGLKernel;

mod d3d11;
mod hook;
mod ipc;
mod wgl;
mod win32;
mod common;
mod kernel;

unsafe fn main() -> Result<(), Box<dyn Error>> {

    let ipc = IpcConnectionBuilder::new(Uuid::nil());
    let ipc = ipc.connect()?;

    let handle = ipc.handle();

    let imgui = Arc::new(RwLock::new(Context::create()));

    let context = KernelContext {
        imgui: imgui.clone(),
        ipc: handle.clone()
    };

    let mut dx11 = Direct3D11Kernel::new(context.clone())?;
    dx11.init()?;
    println!("[dx11] init finish");

    let mut wgl = WGLKernel::new(context)?;
    wgl.init()?;
    println!("[wgl] init finish");

    ipc.listen()?;
    eprintln!("[ipc] ipc stop");
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
