use crate::{hook_define, hook_make_chain, HookHandle};
use ash::prelude::VkResult;
use ash::vk::{PFN_vkCreateSwapchainKHR, StaticFn, SwapchainKHR};
use ash::{vk, RawPtr};
use std::error::Error;
use ash::extensions::khr::Swapchain;
use tokio::io::AsyncReadExt;
use crate::vk::sys::HookedVulkanDeviceHandle;

// https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/vkCreateSwapchainKHR.html
pub type FnCreateSwapchainKHRHook = Box<
    dyn (Fn(
            vk::Device,
            &vk::SwapchainCreateInfoKHR,
            Option<&vk::AllocationCallbacks>,
            CreateSwapchainKHRContext,
        ) -> VkResult<SwapchainKHR>)
        + Send
        + Sync,
>;

struct VkHookHandle {
    create_swapchain_handle: usize,
}

hook_define!(pub chain CREATE_SWAPCHAIN_KHR_CHAIN with FnCreateSwapchainKHRHook => CreateSwapchainKHRContext);

pub(in crate::vk) struct VkHookContext;

impl VkHookContext {
    pub(in crate::vk) fn create_swapchain_khr(
        device: vk::Device,
        create_info: &vk::SwapchainCreateInfoKHR,
        alloc: Option<&vk::AllocationCallbacks>,
    ) -> VkResult<SwapchainKHR> {
        if let Ok(chain) = CREATE_SWAPCHAIN_KHR_CHAIN.read() {
            if let Some((_, next)) = chain.last() {
                let mut iter = chain.iter().rev();
                iter.next();
                return next(
                    device,
                    create_info,
                    alloc,
                    CreateSwapchainKHRContext { chain: iter },
                );
            }
        }

        unsafe {
            let device_vtable = device.get_device_vtable()
                .expect("[vk] device not loaded");
            let instance_vtable = device.get_instance_vtable()
                .expect("[vk] instance not loaded");

            Swapchain::new(&instance_vtable, &device_vtable)
                .create_swapchain(create_info, alloc)
        }
    }

    pub fn init() -> Result<Self, Box<dyn Error>> {
        CREATE_SWAPCHAIN_KHR_CHAIN.write()?.insert(
            0,
            Box::new(move |device, create_info, alloc, _next| unsafe {
                let device_vtable = device.get_device_vtable()
                    .expect("[vk] device not loaded");
                let instance_vtable = device.get_instance_vtable()
                    .expect("[vk] instance not loaded");

                Swapchain::new(&instance_vtable, &device_vtable).create_swapchain(create_info, alloc)
            }),
        );
        Ok(VkHookContext)
    }

    pub fn new(
        &self,
        create_swapchain_khr: FnCreateSwapchainKHRHook,
    ) -> Result<impl HookHandle, Box<dyn Error>> {
        let key = &*create_swapchain_khr as *const _ as *const () as usize;
        CREATE_SWAPCHAIN_KHR_CHAIN
            .write()?
            .insert(key, create_swapchain_khr);
        Ok(VkHookHandle {
            create_swapchain_handle: key,
        })
    }
}

impl Drop for VkHookHandle {
    fn drop(&mut self) {
        CREATE_SWAPCHAIN_KHR_CHAIN
            .write()
            .unwrap()
            .remove(&self.create_swapchain_handle);
    }
}

impl HookHandle for VkHookHandle {}
