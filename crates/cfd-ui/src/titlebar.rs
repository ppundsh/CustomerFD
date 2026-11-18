//! Custom title bar (client area extended over the caption; DWM draws the caption buttons).

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmDefWindowProc;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, IsZoomed, NCCALCSIZE_PARAMS, SM_CXPADDEDBORDER, SM_CYCAPTION, SM_CYSIZEFRAME,
};

pub const WM_NCCALCSIZE: u32 = 0x0083;
pub const WM_NCHITTEST: u32 = 0x0084;
const HTCLIENT: isize = 1;
const HTCAPTION: isize = 2;
const HTTOP: isize = 12;

/// Height (physical px) of the top resize band.
pub unsafe fn resize_frame_h(hwnd: HWND) -> i32 {
    let dpi = GetDpiForWindow(hwnd).max(96);
    GetSystemMetricsForDpi(SM_CYSIZEFRAME, dpi) + GetSystemMetricsForDpi(SM_CXPADDEDBORDER, dpi)
}

/// Height (physical px) of the custom title bar: at least `base_h` design px (48, Task Manager
/// style) and never less than the system caption, so DWM's caption buttons fit.
pub unsafe fn titlebar_h(hwnd: HWND, base_h: i32) -> i32 {
    let dpi = GetDpiForWindow(hwnd).max(96);
    let system = GetSystemMetricsForDpi(SM_CYCAPTION, dpi) + resize_frame_h(hwnd);
    system.max(crate::scale(base_h, dpi))
}

/// Handles the messages that make the custom title bar work. Call first in the window
/// procedure; `Some(result)` means the message was consumed.
pub unsafe fn handle_frame_message(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    base_h: i32,
) -> Option<LRESULT> {
    let mut dwm_result = LRESULT(0);
    if DwmDefWindowProc(hwnd, msg, wparam, lparam, &mut dwm_result).as_bool() {
        return Some(dwm_result);
    }
    match msg {
        WM_NCCALCSIZE if wparam.0 != 0 => {
            let params = &mut *(lparam.0 as *mut NCCALCSIZE_PARAMS);
            let top = params.rgrc[0].top;
            let result = DefWindowProcW(hwnd, msg, wparam, lparam);
            params.rgrc[0].top = top;
            if IsZoomed(hwnd).as_bool() {
                params.rgrc[0].top += resize_frame_h(hwnd);
            }
            Some(result)
        }
        WM_NCHITTEST => {
            let hit = DefWindowProcW(hwnd, msg, wparam, lparam);
            if hit.0 != HTCLIENT {
                return Some(hit);
            }
            let mut pt = POINT {
                x: (lparam.0 & 0xFFFF) as i16 as i32,
                y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
            };
            let _ = ScreenToClient(hwnd, &mut pt);
            if !IsZoomed(hwnd).as_bool() && pt.y < resize_frame_h(hwnd) {
                return Some(LRESULT(HTTOP));
            }
            if pt.y < titlebar_h(hwnd, base_h) {
                return Some(LRESULT(HTCAPTION));
            }
            Some(hit)
        }
        _ => None,
    }
}
