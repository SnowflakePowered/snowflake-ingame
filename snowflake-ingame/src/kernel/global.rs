use std::error::Error;
use std::lazy::SyncOnceCell;
use std::sync::Arc;
use imgui::Context;
use parking_lot::RwLock;
use uuid::Uuid;
use crate::{IpcConnectionBuilder, KernelContext};
use crate::common::RenderError;
use crate::ipc::IpcConnection;

static KERNEL_CONTEXT: SyncOnceCell<KernelContext> = SyncOnceCell::new();
static mut IPC_CONNECTION: SyncOnceCell<IpcConnection> = SyncOnceCell::new();

/// Acquire a handle to the global kernel.
/// This will initialize the IPC connection and ImGui context
/// the first time it is called.
///
/// SAFETY: Calling `kernel::acquire` after `kernel::start` is undefined behaviour.
pub unsafe fn acquire() -> Result<&'static KernelContext, Box<dyn Error>> {
    let ipc = unsafe {
        IPC_CONNECTION.get_or_try_init::<_, Box<dyn Error>>(move || {
            let ipc = IpcConnectionBuilder::new(Uuid::nil()).connect()?;
            Ok(ipc)
        })?
    };

    let context = KERNEL_CONTEXT.get_or_init(|| {
        let handle = ipc.handle();
        let imgui = Arc::new(RwLock::new(Context::create()));

        KernelContext {
            imgui: imgui.clone(),
            ipc: handle.clone()
        }
    });

    Ok(context)
}

/// Start the kernel. Once this function returns, the calling thread must halt.
pub fn start() -> Result<(), Box<dyn Error>> {
    eprintln!("[ipc] starting main ipc loop");
    let ipc = unsafe { IPC_CONNECTION.take().ok_or(RenderError::KernelNotReady)? };
    ipc.listen()?;
    eprintln!("[ipc] stopping main loop");
    Ok(())
}