mod layer;
mod hooks;

use ash::{Device, Instance, vk};

pub trait HookedVulkanDeviceHandle {
    unsafe fn get_device_vtable(&self) -> Option<Device>;
    unsafe fn get_instance_vtable(&self) -> Option<Instance>;
}

impl HookedVulkanDeviceHandle for vk::Device {
    unsafe fn get_device_vtable(&self) -> Option<Device> {
        layer::get_device_vtable(self)
    }

    unsafe fn get_instance_vtable(&self) -> Option<Instance> {
        layer::get_instance_vtable(self)
    }
}