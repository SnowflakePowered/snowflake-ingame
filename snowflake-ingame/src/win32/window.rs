use std::mem::size_of;
use windows::core::PCSTR;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleA;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExA, DefWindowProcA, DestroyWindow, RegisterClassExA, UnregisterClassA, CS_HREDRAW,
    CS_VREDRAW, WNDCLASSEXA, WS_OVERLAPPEDWINDOW,
};

pub struct TempWindow<'a>(WNDCLASSEXA, HWND, &'a [u8]);

unsafe extern "system" fn def_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    DefWindowProcA(hwnd, msg, wparam, lparam)
}

fn get_window_class(class_name: *const u8) -> (WNDCLASSEXA, HWND) {
    unsafe {
        let window_class = WNDCLASSEXA {
            cbSize: size_of::<WNDCLASSEXA>() as _,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(def_wnd_proc),
            hInstance: GetModuleHandleA(None)
                .expect("[w32] Could not get handle to current process."),
            lpszClassName: PCSTR(class_name),
            ..Default::default()
        };

        RegisterClassExA(&window_class);

        let hwnd = CreateWindowExA(
            Default::default(),
            window_class.lpszClassName,
            PCSTR(b"TEMPWINCLS\0".as_ptr()),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            256,
            256,
            None,
            None,
            window_class.hInstance,
            None,
        );

        (window_class, hwnd)
    }
}

impl TempWindow<'_> {
    pub fn new(class_name: &[u8]) -> TempWindow {
        let (wclass, hwnd) = get_window_class(class_name.as_ptr());
        TempWindow(wclass, hwnd, class_name)
    }
}

impl Into<HWND> for &TempWindow<'_> {
    fn into(self) -> HWND {
        self.1
    }
}

impl Drop for TempWindow<'_> {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.1);
            UnregisterClassA(self.0.lpszClassName, self.0.hInstance);
        }
    }
}
