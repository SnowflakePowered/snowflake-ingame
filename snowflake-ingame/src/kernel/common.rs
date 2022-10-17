use crate::ipc::IpcHandle;
use crate::HookHandle;
use parking_lot::RwLock;
use std::error::Error;
use std::mem::ManuallyDrop;
use std::sync::Arc;

#[derive(Clone)]
pub struct KernelContext {
    pub ipc: IpcHandle,
    pub imgui: Arc<RwLock<imgui::Context>>,
}
unsafe impl Sync for KernelContext {}
unsafe impl Send for KernelContext {}

/// All hooks are driven by the FrameKernel at a frame-level granularity.
pub trait FrameKernel
where
    Self: Sized,
    Self::Handle: HookHandle,
{
    /// The drop handle for the hook.
    type Handle;

    /// Create a new handle.
    fn new(context: KernelContext) -> Result<Self, Box<dyn Error>>;

    /// Initialize the kernel hook. The hook should deactivate when the returned
    /// `ManuallyDrop<Self::Handle>`is dropped.
    fn init(&mut self) -> Result<ManuallyDrop<Self::Handle>, Box<dyn Error>>;
}
