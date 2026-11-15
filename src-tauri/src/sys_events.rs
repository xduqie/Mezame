//! Listens for the two things the old idle-timer approach could not see:
//!   * the display turning on/off  (GUID_CONSOLE_DISPLAY_STATE)
//!   * the session being locked/unlocked (WTS_SESSION_LOCK / UNLOCK)
//! A hidden window on its own thread receives the messages. The FFI is declared
//! by hand so it does not depend on any `windows` crate version.

use crate::SysEvent;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use tauri::AppHandle;

#[allow(non_snake_case, non_camel_case_types)]
mod ffi {
    use std::ffi::c_void;
    pub type HWND = *mut c_void;
    pub type HINSTANCE = *mut c_void;
    pub type WPARAM = usize;
    pub type LPARAM = isize;
    pub type LRESULT = isize;
    pub type WNDPROC = Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>;

    #[repr(C)]
    pub struct WNDCLASSW {
        pub style: u32,
        pub lpfnWndProc: WNDPROC,
        pub cbClsExtra: i32,
        pub cbWndExtra: i32,
        pub hInstance: HINSTANCE,
        pub hIcon: *mut c_void,
        pub hCursor: *mut c_void,
        pub hbrBackground: *mut c_void,
        pub lpszMenuName: *const u16,
        pub lpszClassName: *const u16,
    }
    #[repr(C)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }
    #[repr(C)]
    pub struct MSG {
        pub hwnd: HWND,
        pub message: u32,
        pub wParam: WPARAM,
        pub lParam: LPARAM,
        pub time: u32,
        pub pt: POINT,
    }
    #[repr(C)]
    #[derive(PartialEq)]
    pub struct GUID {
        pub data1: u32,
        pub data2: u16,
        pub data3: u16,
        pub data4: [u8; 8],
    }
    #[repr(C)]
    pub struct POWERBROADCAST_SETTING {
        pub PowerSetting: GUID,
        pub DataLength: u32,
        pub Data: [u8; 1],
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn RegisterClassW(class: *const WNDCLASSW) -> u16;
        pub fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: HWND,
            menu: *mut c_void,
            instance: HINSTANCE,
            param: *mut c_void,
        ) -> HWND;
        pub fn DefWindowProcW(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
        pub fn GetMessageW(msg: *mut MSG, hwnd: HWND, min: u32, max: u32) -> i32;
        pub fn TranslateMessage(msg: *const MSG) -> i32;
        pub fn DispatchMessageW(msg: *const MSG) -> LRESULT;
        pub fn RegisterPowerSettingNotification(
            recipient: *mut c_void,
            guid: *const GUID,
            flags: u32,
        ) -> *mut c_void;
    }
    #[link(name = "wtsapi32")]
    extern "system" {
        pub fn WTSRegisterSessionNotification(hwnd: HWND, flags: u32) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        pub fn GetModuleHandleW(name: *const u16) -> HINSTANCE;
    }
}
use ffi::*;

const WM_POWERBROADCAST: u32 = 0x0218;
const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const PBT_POWERSETTINGCHANGE: usize = 0x8013;
const WTS_SESSION_LOCK: usize = 7;
const WTS_SESSION_UNLOCK: usize = 8;

// {6FE69556-704A-47A0-8F24-C28D936FDA47}
const GUID_CONSOLE_DISPLAY_STATE: GUID = GUID {
    data1: 0x6FE69556,
    data2: 0x704A,
    data3: 0x47A0,
    data4: [0x8F, 0x24, 0xC2, 0x8D, 0x93, 0x6F, 0xDA, 0x47],
};

static APP: OnceLock<AppHandle> = OnceLock::new();

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_POWERBROADCAST if w == PBT_POWERSETTINGCHANGE => {
            let s = &*(l as *const POWERBROADCAST_SETTING);
            if s.PowerSetting == GUID_CONSOLE_DISPLAY_STATE && s.DataLength >= 4 {
                // 0 = off, 1 = on, 2 = dimmed
                let state = std::ptr::read_unaligned(s.Data.as_ptr() as *const u32);
                match state {
                    0 => emit(SysEvent::DisplayOff),
                    1 => emit(SysEvent::DisplayOn),
                    _ => {}
                }
            }
            1
        }
        WM_WTSSESSION_CHANGE => {
            match w {
                WTS_SESSION_LOCK => emit(SysEvent::Lock),
                WTS_SESSION_UNLOCK => emit(SysEvent::Unlock),
                _ => {}
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}

fn emit(ev: SysEvent) {
    if let Some(app) = APP.get() {
        crate::on_system_event(app, ev);
    }
}

pub fn start(app: AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| unsafe { run() });
}

unsafe fn run() {
    let hinst = GetModuleHandleW(null());
    let class = wide("MezameEventSink");
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: null_mut(),
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    if RegisterClassW(&wc) == 0 {
        eprintln!("[mezame] RegisterClassW failed");
        return;
    }
    // A normal (never shown) top-level window: message-only windows don't reliably
    // receive broadcast/power notifications.
    let name = wide("Mezame");
    let hwnd = CreateWindowExW(
        0, class.as_ptr(), name.as_ptr(), 0, 0, 0, 0, 0,
        null_mut(), null_mut(), hinst, null_mut(),
    );
    if hwnd.is_null() {
        eprintln!("[mezame] CreateWindowExW failed");
        return;
    }
    if WTSRegisterSessionNotification(hwnd, 0) == 0 {
        eprintln!("[mezame] WTSRegisterSessionNotification failed");
    }
    if RegisterPowerSettingNotification(hwnd as *mut c_void, &GUID_CONSOLE_DISPLAY_STATE, 0).is_null() {
        eprintln!("[mezame] RegisterPowerSettingNotification failed");
    }
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}