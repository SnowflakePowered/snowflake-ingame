use std::error::Error;

use crate::hook::HookHandle;
use detour::static_detour;
use windows::Win32::Foundation::BOOL;
use windows::Win32::Graphics::Gdi::HDC;

use crate::hook_define;
use crate::hook_impl_fn;
use crate::hook_key;
use crate::hook_link_chain;

pub(in crate::wgl) struct WGLHookContext;

struct WGLHookHandle {
    swap_buffers_handle: usize,
}
static_detour! {
    static SWAP_BUFFERS_DETOUR: extern "system" fn(windows::Win32::Graphics::Gdi::HDC) -> windows::Win32::Foundation::BOOL;
}

pub type FnSwapBuffersHook = Box<dyn (Fn(HDC, SwapBuffersContext) -> BOOL) + Send + Sync>;
hook_define!(chain SWAP_BUFFERS_CHAIN with FnSwapBuffersHook => SwapBuffersContext);

impl WGLHookContext {
    hook_impl_fn!(fn swap_buffers(hdc: HDC) -> BOOL =>
        (SWAP_BUFFERS_CHAIN, SWAP_BUFFERS_DETOUR, SwapBuffersContext)
    );

    pub fn init(
        swap_buffers: extern "system" fn(HDC) -> BOOL,
    ) -> Result<WGLHookContext, Box<dyn Error>> {
        // Setup call chain termination before detouring
        hook_link_chain! {
            box link SWAP_BUFFERS_CHAIN with SWAP_BUFFERS_DETOUR => hdc;
        }

        unsafe {
            SWAP_BUFFERS_DETOUR
                .initialize(swap_buffers, WGLHookContext::swap_buffers)?
                .enable()?;
        }

        Ok(WGLHookContext)
    }

    pub fn new(&self, swap_buffers: FnSwapBuffersHook) -> Result<impl HookHandle, Box<dyn Error>> {
        let key = hook_key!(box swap_buffers);
        SWAP_BUFFERS_CHAIN.write()?.insert(key, swap_buffers);

        Ok(WGLHookHandle {
            swap_buffers_handle: key,
        })
    }
}

impl HookHandle for WGLHookHandle {}

impl Drop for WGLHookHandle {
    fn drop(&mut self) {
        SWAP_BUFFERS_CHAIN
            .write()
            .unwrap()
            .remove(&self.swap_buffers_handle);
    }
}
