use std::error::Error;
use ash::{RawPtr, vk};
use ash::vk::{PFN_vkCreateSwapchainKHR, StaticFn};
use tokio::io::AsyncReadExt;
use crate::{hook_define, hook_make_chain, HookHandle};

// https://registry.khronos.org/vulkan/specs/1.3-extensions/man/html/vkCreateSwapchainKHR.html
pub type FnCreateSwapchainKHRHook = Box<dyn (Fn(vk::Device, &vk::SwapchainCreateInfoKHR,
    Option<&vk::AllocationCallbacks>, &mut vk::SwapchainKHR, CreateSwapchainKHRContext) -> vk::Result) + Send + Sync>;

struct VkHookHandle {
    create_swapchain_handle: usize,
}

hook_define!(pub chain CREATE_SWAPCHAIN_KHR_CHAIN with FnCreateSwapchainKHRHook => CreateSwapchainKHRContext);

pub(in crate::vk) struct VkHookContext;

impl VkHookContext {
    pub(in crate::vk) fn create_swapchain_khr(base: vk::PFN_vkCreateSwapchainKHR, device: vk::Device, create_info: &vk::SwapchainCreateInfoKHR,
                            alloc: Option<&vk::AllocationCallbacks>, swapchain: &mut vk::SwapchainKHR) -> vk::Result {
        if let Ok(chain) = CREATE_SWAPCHAIN_KHR_CHAIN.read() {
            if let Some((_, next)) = chain.last() {
                let mut iter = chain.iter().rev();
                iter.next();
                return next(device, create_info, alloc, swapchain, CreateSwapchainKHRContext { chain: iter })
            }
        }
        unsafe {
            base(device, create_info, alloc.as_raw_ptr(), swapchain)
        }
    }

    pub fn init() -> Result<Self, Box<dyn Error>> {
        CREATE_SWAPCHAIN_KHR_CHAIN.write()?
            .insert(0, Box::new(move |device, createinfo, alloc, out, _next| {
                unsafe {
                    let dpa = crate::vk::sys::get_base_device_proc_addr(&device)
                        .expect("[vk] device not loaded");
                    let chr = (dpa)(device, b"vkCreateSwapchainKHR\0".as_ptr().cast());

                    (std::mem::transmute::<_, PFN_vkCreateSwapchainKHR>(chr))(device, createinfo, alloc.as_raw_ptr(), out)
                }
            }));
        Ok(VkHookContext)
    }

    pub fn new(
        &self,
        create_swapchain_khr: FnCreateSwapchainKHRHook,
    ) -> Result<impl HookHandle, Box<dyn Error>> {
        let key = &*create_swapchain_khr as *const _ as *const () as usize;
        CREATE_SWAPCHAIN_KHR_CHAIN.write()?.insert(key, create_swapchain_khr);
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
