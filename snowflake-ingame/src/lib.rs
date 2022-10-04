#![feature(once_cell)]
#![feature(type_alias_impl_trait)]
#![feature(associated_type_defaults)]
#![feature(strict_provenance)]

use std::error::Error;
use std::ffi::c_void;
use std::panic::catch_unwind;
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
        kernel::start()?;
    }
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

        println!("[init] dllmain");
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

use ash::vk::Result as VkResult;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
#[must_use]
pub struct VkLayerNegotiateStructType(pub(crate) i32);
impl VkLayerNegotiateStructType {
    pub const LAYER_NEGOTIATE_INTERFACE_STRUCT: Self = Self(1);
}

#[repr(C)]
pub struct VkNegotiateLayerInterface {
    pub s_type: VkLayerNegotiateStructType,
    pub p_next: *const c_void,
    pub loader_layer_interface_version: u32,
    pub pfn_get_instance_proc_addr: ash::vk::PFN_vkGetInstanceProcAddr,
    pub pfn_get_device_proc_addr:  ash::vk::PFN_vkGetDeviceProcAddr,

    // typedef PFN_vkVoidFunction (VKAPI_PTR *PFN_GetPhysicalDeviceProcAddr)(VkInstance instance, const char* pName);
    pub pfn_get_physical_device_proc_addr: Option<ash::vk::PFN_vkGetInstanceProcAddr>,
}
#[no_mangle]
pub unsafe extern "system" fn vk_main(interface: *mut VkNegotiateLayerInterface) -> VkResult {
    // unsafe { winapi::um::consoleapi::AllocConsole(); }
    println!("[vk] layer version negotiate");
    if (*interface).s_type != VkLayerNegotiateStructType::LAYER_NEGOTIATE_INTERFACE_STRUCT {
        return VkResult::ERROR_INITIALIZATION_FAILED;
    }

    let target_ld = (*interface).loader_layer_interface_version;

    if target_ld < 2 {
        // We only support Layer Interface Version 2.
        return VkResult::ERROR_INITIALIZATION_FAILED;
    }

    // Validate init params
    // if target_ld >= vk_cfg.loader_version {
    //     (*interface).loader_layer_interface_version = vk_cfg.loader_version;
    //     (*interface).pfn_get_device_proc_addr = get_device_proc_addr;
    //     (*interface).pfn_get_instance_proc_addr = get_instance_proc_addr;
    // }
    //
    // (*interface).pfn_get_physical_device_proc_addr = None;

    return VkResult::SUCCESS;
}