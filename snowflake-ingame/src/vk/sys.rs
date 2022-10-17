use crate::vk::hook_vk::VkHookContext;
use crate::{kernel, HookChain};
use ash::extensions::khr::Swapchain;
use ash::vk::{
    AllocationCallbacks, InstanceFnV1_0, Result as VkResult, StaticFn, SwapchainCreateInfoKHR,
    SwapchainKHR,
};
use ash::{vk, Device, Instance};
use std::cell::{LazyCell, OnceCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{OnceLock, RwLock};
use dashmap::DashMap;
use windows::Win32::System::Console::AllocConsole;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
#[must_use]
pub struct VkLayerNegotiateStructType(pub(crate) i32);
impl VkLayerNegotiateStructType {
    pub const LAYER_NEGOTIATE_INTERFACE_STRUCT: Self = Self(1);
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(transparent)]
#[must_use]
pub struct VkLayerFunction(pub(crate) i32);
impl VkLayerFunction {
    pub const VK_LAYER_FUNCTION_LINK: Self = Self(0);
    #[allow(dead_code)]
    pub const VK_LAYER_FUNCTION_DATA_CALLBACK: Self = Self(1);
}

#[repr(C)]
pub struct VkLayerInstanceCreateInfo {
    pub s_type: vk::StructureType,
    pub p_next: *const c_void,
    pub function: VkLayerFunction,
    /* This should properly be represented as union with PFN_vkSetInstanceLoaderData,
      In practice, it doesn't matter.
    */
    pub p_layer_info: *const VkLayerInstanceLink,
}
#[repr(C)]
pub struct VkLayerInstanceLink {
    pub p_next: *const VkLayerInstanceLink,
    pub pfn_next_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub pfn_next_get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

#[repr(C)]
pub struct VkLayerDeviceCreateInfo {
    pub s_type: vk::StructureType,
    pub p_next: *const c_void,
    pub function: VkLayerFunction,
    pub p_layer_info: *const VkLayerDeviceLink,
}

#[repr(C)]
pub struct VkLayerDeviceLink {
    pub p_next: *const VkLayerDeviceLink,
    pub pfn_next_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub pfn_next_get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
}

#[derive(Clone)]
pub struct InstanceDispatchTable {
    pub get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub instance_vtable: Instance,
}

#[derive(Clone)]
pub struct DeviceDispatchTable {
    pub get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,
    pub get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub device_vtable: Device,
    pub instance_vtable: Instance,
    pub physical_device: vk::PhysicalDevice,
}

#[allow(clippy::type_complexity)]
static mut DEVICE: LazyCell<DashMap<vk::Device, DeviceDispatchTable>> =
    LazyCell::new(Default::default);

#[allow(clippy::type_complexity)]
static mut PHYSICAL_DEVICE_MAP: LazyCell<DashMap<vk::PhysicalDevice, vk::Instance>> =
    LazyCell::new(Default::default);

#[allow(clippy::type_complexity)]
static mut INSTANCE: LazyCell<DashMap<vk::Instance, InstanceDispatchTable>> =
    LazyCell::new(Default::default);

#[no_mangle]
unsafe extern "system" fn get_device_proc_addr(
    device: vk::Device,
    p_name: *const std::os::raw::c_char,
) -> vk::PFN_vkVoidFunction {
    let name = CStr::from_ptr(p_name);
    match name.to_bytes() {
        b"vkGetDeviceProcAddr" => Some(std::mem::transmute(
            get_device_proc_addr as vk::PFN_vkGetDeviceProcAddr,
        )),
        b"vkCreateDevice" => Some(std::mem::transmute(create_device as vk::PFN_vkCreateDevice)),
        b"vkDestroyDevice" => Some(std::mem::transmute(
            destroy_device as vk::PFN_vkDestroyDevice,
        )),
        b"vkCreateSwapchainKHR" => Some(std::mem::transmute(
            create_swapchain as vk::PFN_vkCreateSwapchainKHR,
        )),
        _ => DEVICE
            .get(&device)
            .map(|dispatch| (dispatch.get_device_proc_addr)(device, p_name))
            .unwrap_or(None),
    }
}

#[no_mangle]
unsafe extern "system" fn get_instance_proc_addr(
    instance: vk::Instance,
    p_name: *const std::os::raw::c_char,
) -> vk::PFN_vkVoidFunction {
    let name = CStr::from_ptr(p_name);
    match name.to_bytes() {
        b"vkGetInstanceProcAddr" => Some(std::mem::transmute(
            get_instance_proc_addr as vk::PFN_vkGetInstanceProcAddr,
        )),
        b"vkCreateInstance" => Some(std::mem::transmute(
            create_instance as vk::PFN_vkCreateInstance,
        )),
        b"vkDestroyInstance" => Some(std::mem::transmute(
            destroy_instance as vk::PFN_vkDestroyInstance,
        )),
        b"vkGetDeviceProcAddr" => Some(std::mem::transmute(
            get_device_proc_addr as vk::PFN_vkGetDeviceProcAddr,
        )),
        b"vkCreateDevice" => Some(std::mem::transmute(create_device as vk::PFN_vkCreateDevice)),
        b"vkDestroyDevice" => Some(std::mem::transmute(
            destroy_device as vk::PFN_vkDestroyDevice,
        )),
        _ => INSTANCE
            .get(&instance)
            .map(|dispatch| (dispatch.get_instance_proc_addr)(instance, p_name))
            .unwrap_or(None),
    }
}

#[no_mangle]
unsafe extern "system" fn get_base_device_proc_addr(
    device: vk::Device,
    p_name: *const std::os::raw::c_char,
) -> vk::PFN_vkVoidFunction {
    let name = CStr::from_ptr(p_name);
    match name.to_bytes() {
        b"vkGetDeviceProcAddr" => Some(std::mem::transmute(
            get_base_device_proc_addr as vk::PFN_vkGetDeviceProcAddr,
        )),
        _ => DEVICE
            .get(&device)
            .map(|dispatch| (dispatch.get_device_proc_addr)(device, p_name))
            .unwrap_or(None),
    }
}

#[no_mangle]
unsafe extern "system" fn get_base_instance_proc_addr(
    instance: vk::Instance,
    p_name: *const std::os::raw::c_char,
) -> vk::PFN_vkVoidFunction {
    let name = CStr::from_ptr(p_name);
    match name.to_bytes() {
        b"vkGetInstanceProcAddr" => Some(std::mem::transmute(
            get_base_instance_proc_addr as vk::PFN_vkGetInstanceProcAddr,
        )),
        b"vkGetDeviceProcAddr" => Some(std::mem::transmute(
            get_base_device_proc_addr as vk::PFN_vkGetDeviceProcAddr,
        )),
        _ => INSTANCE
            .get(&instance)
            .map(|dispatch| (dispatch.get_instance_proc_addr)(instance, p_name))
            .unwrap_or(None),
    }
}

pub unsafe fn get_device_vtable(device_handle: &vk::Device) -> Option<Device> {
    if let Some(device) = DEVICE.get(device_handle) {
        let instance = &device.instance_vtable;
        let device = Device::load(instance.fp_v1_0(), *device_handle);
        return Some(device);
    }
    None
}

pub unsafe fn get_swapchain_vtable(device_handle: &vk::Device) -> Option<Swapchain> {
    if let Some(device) = DEVICE.get(device_handle) {
        let instance = &device.instance_vtable;
        let device = Device::load(instance.fp_v1_0(), *device_handle);
        return Some(Swapchain::new(&instance, &device));
    }
    None
}

unsafe extern "system" fn create_swapchain(
    device: vk::Device,
    create_info: *const SwapchainCreateInfoKHR,
    allocator: *const AllocationCallbacks,
    swapchain: *mut SwapchainKHR,
) -> vk::Result {
    let swapchain_fp =
        get_swapchain_vtable(&device).expect("[vk] could not get swapchain extensions.");
    VkHookContext::create_swapchain_khr(
        swapchain_fp.fp().create_swapchain_khr,
        device,
        &*create_info,
        allocator.as_ref(),
        &mut *swapchain,
    )
}

// https://android.googlesource.com/platform/cts/+/6743db1/hostsidetests/gputools/layers/jni/nullLayer.cpp
unsafe extern "system" fn create_device(
    physical_device: vk::PhysicalDevice,
    p_create_info: *const vk::DeviceCreateInfo,
    p_allocator: *const vk::AllocationCallbacks,
    p_device: *mut vk::Device,
) -> vk::Result {
    println!("[vk] create_device");

    let instance_info = p_create_info.as_ref().unwrap();

    let mut layer_info = instance_info
        .p_next
        .cast::<VkLayerDeviceCreateInfo>()
        .cast_mut();
    while !layer_info.is_null()
        && ((*layer_info).s_type != vk::StructureType::LOADER_DEVICE_CREATE_INFO
            || (*layer_info).function != VkLayerFunction::VK_LAYER_FUNCTION_LINK)
    {
        // I have no idea if this is safe lol
        layer_info = (*layer_info)
            .p_next
            .cast::<VkLayerDeviceCreateInfo>()
            .cast_mut()
    }

    if layer_info.is_null() {
        return VkResult::ERROR_INITIALIZATION_FAILED;
    }

    // Don't move link
    let next_layer_info = (*layer_info).p_layer_info.read();

    let gipa = next_layer_info.pfn_next_get_instance_proc_addr;
    let gdpa = next_layer_info.pfn_next_get_device_proc_addr;

    // move chain on for next layer
    // this is so bad.
    (*layer_info).p_layer_info = next_layer_info.p_next;

    let fp_create_device: vk::PFN_vkCreateDevice = std::mem::transmute(gipa(
        vk::Instance::null(),
        b"vkCreateDevice\0".as_ptr() as *const c_char,
    ));
    let result = fp_create_device(physical_device, p_create_info, p_allocator, p_device);

    let instance_handle = PHYSICAL_DEVICE_MAP.get(&physical_device)
        .expect("[vk] no instance found for physical device")
        .clone();

    // the unhooked instance vtable isn't actually used,
    // except for get_device_proc_addr.
    let entry = StaticFn {
        get_instance_proc_addr: get_base_instance_proc_addr,
    };
    let instance = Instance::load(&entry, instance_handle);

    // This is important to not rely on the layer-local GetDeviceProcAddress.
    let mut instance_vtable = instance.fp_v1_0().clone();
    instance_vtable.get_device_proc_addr = gdpa;

    let device_vtable = Device::load(&instance_vtable, *p_device);

    let dispatch = DeviceDispatchTable {
        get_device_proc_addr: gdpa,
        get_instance_proc_addr: gipa,
        device_vtable,
        instance_vtable: instance,
        physical_device,
    };

    let result = (move || {
        DEVICE.insert(*p_device, dispatch);
        kernel::acquire().ok()?;
        Some(result)
    })()
    .unwrap_or(VkResult::ERROR_INITIALIZATION_FAILED);

    std::thread::spawn(|| {
        println!("[vk] starting kernel");
        kernel::start().expect("kernel failed to start");
    });

    return result;
}

unsafe extern "system" fn destroy_device(
    device: vk::Device,
    p_allocator: *const vk::AllocationCallbacks,
) {
    // todo: delete kernel...
    (|| {
        kernel::kill();
        let dispatch = DEVICE.remove(&device);
        if let Some((_, dispatch)) = dispatch {
            dispatch.device_vtable.destroy_device(p_allocator.as_ref())
        }
        Some(())
    })()
    .unwrap_or(())
}

unsafe extern "system" fn create_instance(
    p_create_info: *const vk::InstanceCreateInfo,
    p_allocator: *const vk::AllocationCallbacks,
    p_instance: *mut vk::Instance,
) -> vk::Result {
    println!("[vk] create_instance");
    let instance_info = p_create_info.as_ref().unwrap();

    let mut layer_info = instance_info
        .p_next
        .cast::<VkLayerInstanceCreateInfo>()
        .cast_mut();
    while !layer_info.is_null()
        && ((*layer_info).s_type != vk::StructureType::LOADER_INSTANCE_CREATE_INFO
            || (*layer_info).function != VkLayerFunction::VK_LAYER_FUNCTION_LINK)
    {
        // I have no idea if this is safe lol
        layer_info = (*layer_info)
            .p_next
            .cast::<VkLayerInstanceCreateInfo>()
            .cast_mut()
    }

    if layer_info.is_null() {
        return VkResult::ERROR_INITIALIZATION_FAILED;
    }

    // Don't move link
    let next_layer_info = (*layer_info).p_layer_info.read();

    // move chain on for next layer
    // this is so bad.
    (*layer_info).p_layer_info = next_layer_info.p_next;

    let gpa = next_layer_info.pfn_next_get_instance_proc_addr;

    // hippity hoppity your PFN_vkVoidFunction is now a PFN_vkCreateInstance
    let fp_create_instance: vk::PFN_vkCreateInstance = std::mem::transmute(gpa(
        vk::Instance::null(),
        b"vkCreateInstance\0".as_ptr() as *const c_char,
    ));

    let result = fp_create_instance(p_create_info, p_allocator, p_instance);

    let instance_vtable = Instance::load(
        &StaticFn {
            get_instance_proc_addr: gpa,
        },
        *p_instance,
    );

    if let Ok(phys_devices) = instance_vtable.enumerate_physical_devices() {
        for device in phys_devices {
            PHYSICAL_DEVICE_MAP.insert(device, *p_instance);
        }
    }

    let dispatch = InstanceDispatchTable {
        get_instance_proc_addr: gpa,
        instance_vtable,
    };

    let result = (move || {
        INSTANCE.insert(*p_instance, dispatch);
        Some(result)
    })()
    .unwrap_or(VkResult::ERROR_INITIALIZATION_FAILED);

    return result;
}

unsafe extern "system" fn destroy_instance(
    instance: vk::Instance,
    p_allocator: *const vk::AllocationCallbacks,
) {
    (|| {
        let dispatch = INSTANCE.remove(&instance);
        if let Some((_, dispatch)) = dispatch {
            dispatch
                .instance_vtable
                .destroy_instance(p_allocator.as_ref())
        }
        Some(())
    })()
    .unwrap_or(())
}

#[repr(C)]
pub struct VkNegotiateLayerInterface {
    pub s_type: VkLayerNegotiateStructType,
    pub p_next: *const c_void,
    pub loader_layer_interface_version: u32,
    pub pfn_get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr,
    pub pfn_get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr,

    // typedef PFN_vkVoidFunction (VKAPI_PTR *PFN_GetPhysicalDeviceProcAddr)(VkInstance instance, const char* pName);
    pub pfn_get_physical_device_proc_addr: Option<ash::vk::PFN_vkGetInstanceProcAddr>,
}

use crate::hook::HookHandle;

#[no_mangle]
pub unsafe extern "system" fn vk_main(interface: *mut VkNegotiateLayerInterface) -> VkResult {
    eprintln!("[vk] layer version negotiate");
    if (*interface).s_type != VkLayerNegotiateStructType::LAYER_NEGOTIATE_INTERFACE_STRUCT {
        return VkResult::ERROR_INITIALIZATION_FAILED;
    }

    let target_ld = (*interface).loader_layer_interface_version;

    if target_ld < 2 {
        // We only support Layer Interface Version 2.
        return VkResult::ERROR_INITIALIZATION_FAILED;
    }

    // Validate init params
    if target_ld >= 2 {
        (*interface).loader_layer_interface_version = 2;
        (*interface).pfn_get_device_proc_addr = get_device_proc_addr;
        (*interface).pfn_get_instance_proc_addr = get_instance_proc_addr;
    }

    (*interface).pfn_get_physical_device_proc_addr = None;
    VkHookContext::init()
        .expect("todo panic init")
        .new(Box::new(move |device, create_info, all, s, mut next| {
            eprintln!("{:?}", *create_info);
            let fp = next.fp_next();
            fp(device, create_info, all, s, next)
        }))
        .expect("TODO: panic message")
        .persist();
    return VkResult::SUCCESS;
}
