#![feature(once_cell)]
#![feature(type_alias_impl_trait)]
#![feature(associated_type_defaults)]
#![feature(strict_provenance)]

use std::error::Error;
use std::ffi::c_void;
use std::panic::catch_unwind;
use windows::Win32::Foundation::{BOOL, HINSTANCE};
use windows::Win32::System::Console::AllocConsole;
use windows::Win32::System::LibraryLoader::DisableThreadLibraryCalls;
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
mod vk;

unsafe fn main() -> Result<(), Box<dyn Error>> {
    println!("[ingame] reached main");
    let context = kernel::acquire()?;
    println!("[ingame] kernel acquired");
    let mut dx11 = Direct3D11Kernel::new(context.clone())?;
    dx11.init()?;
    println!("[dx11] init finish");

    let mut wgl = WGLKernel::new(context.clone())?;
    wgl.init()?;
    println!("[wgl] init finish");

    if !vk::entry::is_vk_loaded() {
        println!("[init] starting kernel.");
        kernel::start()?;
    } else {
        println!("[vk] deferring kernel start to Vulkan.")
    }
    Ok(())
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    module: HINSTANCE,
    call_reason: u32,
    _reserved: *mut c_void,
) -> BOOL {
    // disable DLL_THREAD_ATTACH
    unsafe { DisableThreadLibraryCalls(module); }

    if call_reason == DLL_PROCESS_ATTACH {
        unsafe {
            AllocConsole();
        }

        println!("[init] DllMain");
        std::thread::spawn(|| unsafe {
            println!(
                "[init] {:?}",
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
            println!("[init] DllMain over");
        });
    }
    true.into()
}
