use crate::common::RenderError;
use crate::ipc::IpcConnection;
use crate::{IpcConnectionBuilder, KernelContext};
use imgui::Context;
use parking_lot::RwLock;
use std::error::Error;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::oneshot::*;
use uuid::Uuid;

static mut KERNEL_CONTEXT: OnceLock<KernelContext> = OnceLock::new();
static mut IPC_CONNECTION: OnceLock<IpcConnection> = OnceLock::new();
static mut KILL_HANDLE: OnceLock<Sender<()>> = OnceLock::new();
static mut IMGUI_CONTEXT: OnceLock<Arc<RwLock<Context>>> = OnceLock::new();

/// Acquire a handle to the global kernel.
/// This will initialize the IPC connection and ImGui context
/// the first time it is called. This function is safe to call multiple times,
/// if and only if it is not called before `kernel::start`.
///
/// SAFETY: Calling `kernel::acquire` after `kernel::start` is undefined behaviour.
pub unsafe fn acquire() -> Result<&'static KernelContext, Box<dyn Error>> {
    if let Some(context) = KERNEL_CONTEXT.get() {
        println!("[krnl] reusing existing context");
        return Ok(context);
    }

    println!("[krnl] initializing IPC connnection");
    let ipc = unsafe {
        IPC_CONNECTION.get_or_try_init::<_, Box<dyn Error>>(move || {
            let (kill_tx, kill_rx) = channel();
            let ipc = IpcConnectionBuilder::new(Uuid::nil()).connect(Some(kill_rx))?;
            KILL_HANDLE.get_or_init(move || kill_tx);
            Ok(ipc)
        })?
    };

    let imgui = IMGUI_CONTEXT.get_or_init(|| {
        eprintln!("[krnl] initializing imgui context");
        Arc::new(RwLock::new(Context::create()))
    });

    let context = KERNEL_CONTEXT.get_or_init(|| {
        let handle = ipc.handle();
        KernelContext {
            imgui: imgui.clone(),
            ipc: handle.clone(),
        }
    });

    Ok(context)
}

/// Kill a running IPC thread from a different thread.
///
/// This method is idempotent if and only if the IPC connection can be killed or is already
/// dead. If the IPC connection was acquired but not started when this method is called,
/// this will panic and kill the calling thread.
pub fn kill() {
    unsafe {
        if let Some(handle) = KILL_HANDLE.take() {
            handle
                .send(())
                .expect("Could not send kill signal to receiving thread.");
        }

        // SAFETY: This should return None cause it should only be called after start.
        assert!(IPC_CONNECTION.take().is_none());

        // Drop the existing kernel context.
        drop(KERNEL_CONTEXT.take());
    }
}

/// Start the kernel. Once this function returns, the calling thread must halt.
pub fn start() -> Result<(), Box<dyn Error>> {
    eprintln!("[krnl] starting main ipc loop");
    if unsafe { IPC_CONNECTION.get() }.is_none() {
        eprintln!("[krnl] ipc already consumed.");
        return Ok(());
    }
    let ipc = unsafe { IPC_CONNECTION.take().ok_or(RenderError::KernelNotReady)? };
    ipc.listen()?;
    eprintln!("[krnl] stopping main loop");
    Ok(())
}
