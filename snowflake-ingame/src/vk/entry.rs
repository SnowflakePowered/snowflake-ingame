use windows::core::PCSTR;
use windows::Win32::System::LibraryLoader::GetModuleHandleA;
use ash::vk;
use ash::vk::{Handle, Result};

/// Get whether or not Vulkan is loaded.
pub fn is_vk_loaded() -> bool {
    let vk_instance =
        unsafe { GetModuleHandleA(PCSTR(b"vulkan-1\0".as_ptr())) };
    return !vk_instance.is_err()
}
