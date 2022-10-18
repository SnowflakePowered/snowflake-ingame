use ash::vk;
use ash::vk::{AllocationCallbacks, SwapchainCreateInfoKHR, SwapchainKHR};
use ash::vk::Result as VkResult;
use core::result::Result::{Err, Ok};
use crate::vk::hook_vk::VkHookContext;

pub unsafe extern "system" fn create_swapchain(
    device: vk::Device,
    create_info: *const SwapchainCreateInfoKHR,
    allocator: *const AllocationCallbacks,
    p_swapchain: *mut SwapchainKHR,
) -> VkResult {
    let result = VkHookContext::create_swapchain_khr(
        device,
        &*create_info,
        allocator.as_ref(),
    );

    match result {
        Ok(swapchain) => {
            p_swapchain.write(swapchain);
            VkResult::SUCCESS
        }
        Err(r) => r
    }
}
