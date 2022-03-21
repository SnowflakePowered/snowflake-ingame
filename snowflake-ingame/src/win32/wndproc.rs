use std::lazy::SyncLazy;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use dashmap::DashMap;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{CallWindowProcW, DefWindowProcW, GetWindowLongPtrW, GWLP_WNDPROC, SetWindowLongPtrW, WINDOW_LONG_PTR_INDEX, WNDPROC};

#[derive(Debug)]
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
        self.send.try_send(WndProcMsg { hwnd, msg, wparam, lparam })
            .unwrap_or_default();
        if self.block.load(Ordering::SeqCst) {
            DefWindowProcW(hwnd, msg, wparam, lparam)
        } else {
            CallWindowProcW(self.base, hwnd, msg, wparam, lparam)
        }
    }
}

pub struct WndProcHandle {
    block: Arc<AtomicBool>,
    recv: crossbeam_channel::Receiver<WndProcMsg>,
    send: crossbeam_channel::Sender<WndProcMsg>,
    current: WndProcHook
}

#[repr(transparent)]
pub struct WndProcHook {
    hwnd: HWND
}

impl WndProcHook {
    pub unsafe fn new(hwnd: HWND, handle: &WndProcHandle) -> Result<WndProcHook, bool> {
        // this probably isn't 100% safe..
        if WNDPROC_REGISTRY.contains_key(&hwnd.0) {
            return Err(false);
        }

        let old_wndproc = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC,
                                                     wndproc_shim as isize) };
        WNDPROC_REGISTRY.insert(hwnd.0, WndProcRecord {
            base: std::mem::transmute(old_wndproc),
            block: handle.block.clone(),
            send: handle.make_send()
        });

        Ok(WndProcHook {
            hwnd
        })
    }
}

impl Drop for WndProcHook {
    fn drop(&mut self) {
        drop(WNDPROC_REGISTRY.remove(&self.hwnd.0));
    }
}

impl WndProcHandle {
    pub fn new() -> WndProcHandle {
        let (send, recv) = crossbeam_channel::bounded(32);

        WndProcHandle {
            block: Arc::new(AtomicBool::new(false)),
            recv,
            send,
            current: WndProcHook { hwnd: HWND(0) }
        }
    }

    pub fn attach(&mut self, hwnd: HWND) {
        if self.current.hwnd == hwnd {
            return;
        }

        eprintln!("going for new {:x?}", hwnd);
        let (send, recv) = crossbeam_channel::bounded(32);
        self.send = send;
        self.recv = recv;
        let new_wndproc = unsafe { WndProcHook::new(hwnd, &self).unwrap() };
        self.current = new_wndproc;
    }

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

    fn make_send(&self) -> crossbeam_channel::Sender<WndProcMsg> {
        self.send.clone()
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

