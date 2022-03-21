use std::lazy::SyncLazy;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use dashmap::DashMap;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{CallWindowProcW, DefWindowProcW, GetWindowLongPtrW, GWLP_WNDPROC, SetWindowLongPtrW, WINDOW_LONG_PTR_INDEX, WNDPROC};

pub struct WndProcMsg {
    pub hwnd: HWND,
    pub msg: u32,
    pub wparam: WPARAM,
    pub lparam: LPARAM
}

pub struct WndProcRecord {
    base: WNDPROC,
    block: Arc<AtomicBool>,
    send: crossbeam_channel::Sender<WndProcMsg>
}

impl WndProcRecord {
    pub unsafe fn call(&self, hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        eprintln!("WNDPROC {:?} {:x} {:x?} {:x?}", hwnd, msg, wparam, lparam);
        self.send.try_send(WndProcMsg { hwnd, msg, wparam, lparam })
            .unwrap_or_default();
        if self.block.load(Ordering::SeqCst) {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        } else {
            CallWindowProcW(self.base, hwnd, msg, wparam, lparam)
        }
    }
}

#[derive(Clone)]
pub struct WndProcHandle {
    block: Arc<AtomicBool>,
    recv: crossbeam_channel::Receiver<WndProcMsg>
}

#[repr(transparent)]
pub struct WndProcHook {
    hwnd: HWND
}

impl WndProcHook {
    pub unsafe fn new(hwnd: HWND) -> Result<(WndProcHook, WndProcHandle), bool> {
        // this probably isn't 100% safe..
        if WNDPROC_REGISTRY.contains_key(&hwnd.0) {
            return Err(false);
        }

        let block = Arc::new(AtomicBool::new(false));
        let (send, recv) = crossbeam_channel::unbounded();

        let handle = WndProcHandle {
            block: block.clone(),
            recv
        };

        let old_wndproc = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC,
                                                     wndproc_shim as isize) };

        WNDPROC_REGISTRY.insert(hwnd.0, WndProcRecord {
            base: std::mem::transmute(old_wndproc),
            block,
            send
        });

        Ok((WndProcHook {
            hwnd
        }, handle))
    }

    pub fn for_hwnd(&self, hwnd: HWND) -> bool {
        self.hwnd == hwnd
    }
}

impl Drop for WndProcHook {
    fn drop(&mut self) {
        WNDPROC_REGISTRY.remove(&self.hwnd.0);
    }
}

impl WndProcHandle {
    pub fn set_block(&self, block: bool) {
        self.block.store(block, Ordering::SeqCst);
    }

    #[allow(dead_code)]
    pub fn recv(&self) -> Result<WndProcMsg, crossbeam_channel::RecvError> {
        self.recv.recv()
    }

    pub fn try_recv(&self) -> Result<WndProcMsg, crossbeam_channel::TryRecvError> {
        self.recv.try_recv()
    }
}

static WNDPROC_REGISTRY: SyncLazy<DashMap<isize, WndProcRecord>> = SyncLazy::new(|| DashMap::new());

unsafe extern "system" fn wndproc_shim(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Some(wndproc) = WNDPROC_REGISTRY.get(&hwnd.0) {
        wndproc.call(hwnd, msg, wparam, lparam)
    } else {
       DefWindowProcW(hwnd, msg, wparam, lparam)
    }
}

