// CustomerFD — Windows 11 Explorer sidebar custom folders.
// Copyright (C) 2026 ppundsh
// SPDX-License-Identifier: GPL-3.0-or-later

//! CustomerFD control center (`cfd-manager.exe`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case)]

mod app;
mod edit;
mod i18n;
mod pages;
mod render;
mod sys;
mod update_core;
mod updater;

pub use i18n::tr;

use app::{
    handle_action, with_state, Action, AppState, Cursor, Field, STATE, TIMER_ANIM, TIMER_CARET,
    TIMER_INFO,
};
use cfd_core::icon::create_app_hicon;
use cfd_ui::backdrop::apply_frosted_backdrop;
use cfd_ui::scale;
use cfd_ui::theme::{accent_rgb, refresh_system_theme};
use cfd_ui::titlebar::{handle_frame_message, titlebar_h};
use cfd_ui::wstr::to_wide;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const REPO_URL: &str = "https://github.com/ppundsh/CustomerFD";
const CLASS_NAME: PCWSTR = w!("CustomerFDManagerWindow");
const WM_DWMCOLORIZATIONCOLORCHANGED: u32 = 0x0320;
const WM_MOUSELEAVE: u32 = 0x02A3;

fn lparam_xy(l: LPARAM) -> (i32, i32) {
    (
        (l.0 & 0xFFFF) as i16 as i32,
        ((l.0 >> 16) & 0xFFFF) as i16 as i32,
    )
}

unsafe fn invalidate(hwnd: HWND) {
    let _ = InvalidateRect(hwnd, None, false);
}

/// Client point → UI coordinates (y = 0 at the bottom of the title bar).
unsafe fn to_ui(hwnd: HWND, l: LPARAM) -> (i32, i32) {
    let (x, y) = lparam_xy(l);
    (x, y - titlebar_h(hwnd, render::TITLEBAR_H))
}

unsafe fn update_hover(st: &mut AppState) -> bool {
    let new = if st.mouse_inside {
        st.region_at(st.mouse.0, st.mouse.1)
            .map(|r| r.action.clone())
    } else {
        None
    };
    if new != st.hover_action {
        st.hover_action = new;
        true
    } else {
        false
    }
}

unsafe fn on_mouse_down(hwnd: HWND, l: LPARAM) {
    let (x, y) = to_ui(hwnd, l);
    with_state(|st| {
        st.mouse = (x, y);
        st.mouse_inside = true;
        let action = st.region_at(x, y).map(|r| r.action.clone());
        // clicking anywhere except the focused box (or its clear button) leaves the box
        if let Some(f) = st.focused {
            if action != Some(Action::FocusField(f)) && action != Some(Action::ClearField(f)) {
                st.unfocus();
            }
        }
        match &action {
            Some(Action::FocusField(f)) => edit::mouse_down(st, *f, x),
            Some(Action::ScrollThumb) => {
                st.thumb_drag = Some((y, st.scroll_y));
                let _ = SetCapture(hwnd);
            }
            Some(Action::SwitchPage(_)) | Some(Action::NavToggle) | Some(Action::LangButton) => {
                st.nav_press.start();
                st.start_anim();
            }
            Some(Action::SetLang(_)) => {
                st.lang_press.start();
                st.start_anim();
            }
            Some(Action::Expander(id)) => {
                if let Some(e) = st.expanders.get_mut(id) {
                    e.press.start();
                }
                st.start_anim();
            }
            _ => {}
        }
        if !matches!(
            action,
            Some(Action::FocusField(_)) | Some(Action::ScrollThumb)
        ) && action.is_some()
        {
            let _ = SetCapture(hwnd);
        }
        st.pressed = action;
    });
    invalidate(hwnd);
}

unsafe fn on_mouse_up(hwnd: HWND, l: LPARAM) {
    let (x, y) = to_ui(hwnd, l);
    let fire = with_state(|st| {
        st.mouse = (x, y);
        edit::mouse_up(st);
        st.thumb_drag = None;
        let pressed = st.pressed.take();
        let under = st.region_at(x, y).map(|r| r.action.clone());
        match pressed {
            Some(a)
                if Some(&a) == under.as_ref()
                    && !matches!(
                        a,
                        Action::FocusField(_) | Action::ScrollThumb | Action::Block
                    ) =>
            {
                Some(a)
            }
            _ => None,
        }
    })
    .flatten();
    let _ = ReleaseCapture();
    if let Some(a) = fire {
        handle_action(hwnd, a);
    }
    invalidate(hwnd);
}

unsafe fn on_mouse_move(hwnd: HWND, l: LPARAM) {
    let (x, y) = to_ui(hwnd, l);
    let mut tme = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    let _ = TrackMouseEvent(&mut tme);
    let repaint = with_state(|st| {
        st.mouse = (x, y);
        st.mouse_inside = true;
        let mut repaint = update_hover(st) || st.pressed.is_some();
        if let Some((y0, s0)) = st.thumb_drag {
            let track_h = (st.scroll_track.bottom - st.scroll_track.top).max(1);
            let thumb = cfd_ui::widgets::scrollbar_thumb_rect(
                st.dpi,
                &st.scroll_track,
                st.scroll_y,
                st.max_scroll,
            );
            let free = (track_h - (thumb.bottom - thumb.top)).max(1);
            st.scroll_y = (s0 + ((y - y0) as i64 * st.max_scroll as i64 / free as i64) as i32)
                .clamp(0, st.max_scroll);
            repaint = true;
        }
        if st.focused.is_some()
            && st
                .edits
                .get(&st.focused.unwrap())
                .map(|e| e.dragging)
                .unwrap_or(false)
        {
            edit::mouse_drag(st, x);
            repaint = true;
        }
        repaint
    })
    .unwrap_or(false);
    if repaint {
        invalidate(hwnd);
    }
}

unsafe fn set_cursor(hwnd: HWND) -> bool {
    let kind = with_state(|st| {
        if !st.mouse_inside || st.mouse.1 < 0 {
            return None;
        }
        Some(
            st.region_at(st.mouse.0, st.mouse.1)
                .map(|r| r.cursor)
                .unwrap_or(Cursor::Arrow),
        )
    })
    .flatten();
    let Some(kind) = kind else { return false };
    let id = match kind {
        Cursor::Arrow => IDC_ARROW,
        Cursor::Hand => IDC_HAND,
        Cursor::IBeam => IDC_IBEAM,
    };
    if let Ok(c) = LoadCursorW(None, id) {
        SetCursor(c);
    }
    let _ = hwnd;
    true
}

unsafe fn on_key(hwnd: HWND, vk: u32) -> bool {
    enum Next {
        None,
        Act(Action),
        Handled,
    }
    let next = with_state(|st| {
        if st.modal.is_some() {
            return match vk {
                0x1B => Next::Act(Action::ModalCancel),
                0x0D => Next::Act(Action::ModalConfirm),
                _ => Next::Handled,
            };
        }
        if st.lang_open && vk == 0x1B {
            st.lang_open = false;
            return Next::Handled;
        }
        match edit::key_down(st, hwnd, vk) {
            edit::KeyResult::Handled => Next::Handled,
            edit::KeyResult::NotHandled => Next::None,
            edit::KeyResult::Submit(Field::SubName) | edit::KeyResult::Submit(Field::SubPath) => {
                Next::Act(Action::SaveSub)
            }
            edit::KeyResult::Submit(_) => {
                st.unfocus();
                Next::Handled
            }
        }
    })
    .unwrap_or(Next::None);
    match next {
        Next::None => false,
        Next::Handled => {
            invalidate(hwnd);
            true
        }
        Next::Act(a) => {
            handle_action(hwnd, a);
            true
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(r) = handle_frame_message(hwnd, msg, wparam, lparam, render::TITLEBAR_H) {
        return r;
    }
    match msg {
        WM_CREATE => {
            let dpi = GetDpiForWindow(hwnd).max(96);
            let just_updated = std::env::args().any(|a| a == "--updated");
            let st = AppState::new(hwnd, dpi, just_updated);
            STATE.with(|c| *c.borrow_mut() = Some(st));
            LRESULT(0)
        }
        WM_PAINT => {
            render::paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
            let dpi = GetDpiForWindow(hwnd).max(96);
            mmi.ptMinTrackSize.x = scale(880, dpi);
            mmi.ptMinTrackSize.y = scale(620, dpi);
            LRESULT(0)
        }
        WM_DPICHANGED => {
            let dpi = (wparam.0 & 0xFFFF) as u32;
            with_state(|st| {
                st.dpi = dpi.max(96);
                st.icons.clear();
                st.icon_previews.clear();
            });
            let r = &*(lparam.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED | WM_DWMCOLORIZATIONCOLORCHANGED => {
            refresh_system_theme(Some(hwnd));
            apply_frosted_backdrop(hwnd);
            invalidate(hwnd);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEMOVE => {
            on_mouse_move(hwnd, lparam);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            with_state(|st| {
                if st.pressed.is_none() && st.thumb_drag.is_none() {
                    st.mouse_inside = false;
                    st.hover_action = None;
                }
            });
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
            on_mouse_down(hwnd, lparam);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            on_mouse_up(hwnd, lparam);
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as i32;
            with_state(|st| {
                if st.modal.is_none() && !st.lang_open {
                    let step = scale(50, st.dpi);
                    st.scroll_y = (st.scroll_y - delta * step / 120).clamp(0, st.max_scroll);
                }
            });
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if (lparam.0 & 0xFFFF) as u32 == HTCLIENT && set_cursor(hwnd) {
                return LRESULT(1);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_KEYDOWN => {
            if on_key(hwnd, wparam.0 as u32) {
                LRESULT(0)
            } else {
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CHAR => {
            thread_local!(static HIGH: std::cell::Cell<Option<u16>> = const { std::cell::Cell::new(None) });
            let handled = HIGH.with(|h| {
                let mut pending = h.get();
                let r = with_state(|st| edit::char_input(st, wparam.0 as u16, &mut pending))
                    .unwrap_or(false);
                h.set(pending);
                r
            });
            if handled {
                invalidate(hwnd);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            match wparam.0 {
                TIMER_ANIM => {
                    let active = with_state(app::step_animations).unwrap_or(false);
                    if !active {
                        let _ = KillTimer(hwnd, TIMER_ANIM);
                    }
                }
                TIMER_CARET => {
                    with_state(|st| st.caret_visible = !st.caret_visible);
                }
                TIMER_INFO => {
                    let _ = KillTimer(hwnd, TIMER_INFO);
                    with_state(|st| {
                        if st
                            .info
                            .as_ref()
                            .map(|i| {
                                !matches!(
                                    i.severity,
                                    cfd_ui::widgets::Severity::Error
                                        | cfd_ui::widgets::Severity::Warning
                                )
                            })
                            .unwrap_or(false)
                        {
                            st.info = None;
                        }
                    });
                }
                _ => {}
            }
            invalidate(hwnd);
            LRESULT(0)
        }
        m if m == app::WM_APP_SHELL_TRACE => {
            with_state(|st| app::note_shell_trace(st, wparam.0, lparam.0 != 0));
            invalidate(hwnd);
            LRESULT(0)
        }
        m if m == updater::WM_APP_UPDATE => {
            with_state(app::note_update_check);
            invalidate(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            STATE.with(|c| {
                if let Ok(mut g) = c.try_borrow_mut() {
                    *g = None;
                }
            });
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Brings an already running control center to the front; returns true if one exists.
unsafe fn activate_existing() -> bool {
    match FindWindowW(CLASS_NAME, PCWSTR::null()) {
        Ok(h) if !h.is_invalid() => {
            if IsIconic(h).as_bool() {
                let _ = ShowWindow(h, SW_RESTORE);
            }
            let _ = SetForegroundWindow(h);
            true
        }
        _ => false,
    }
}

/// `MAKEINTRESOURCEW(1)`: the application icon resource id.
#[allow(clippy::manual_dangling_ptr)]
const MAKEINTRESOURCE_1: *const u16 = 1 as *const u16;

fn main() {
    // Elevated helper mode (started by the control center through UAC): switch the shell
    // diagnostic trace and exit without any UI.
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args
        .iter()
        .position(|a| a == cfd_core::diagnostics::SET_TRACE_ARG)
    {
        let on = args.get(i + 1).is_some_and(|v| v == "on");
        std::process::exit(match cfd_core::diagnostics::set_shell_trace(on) {
            Ok(()) => 0,
            Err(_) => 1,
        });
    }
    // SAFETY: standard Win32 application start-up on the UI thread.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if !std::env::args().any(|a| a == "--updated") && activate_existing() {
            return;
        }
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        refresh_system_theme(None);

        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let hicon = LoadIconW(hinst, PCWSTR(MAKEINTRESOURCE_1))
            .or_else(|_| create_app_hicon(accent_rgb()).ok_or(()))
            .unwrap_or_default();
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hIcon: hicon,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let dpi = GetDpiForSystem().max(96);
        let (w, h) = (scale(1040, dpi), scale(760, dpi));
        let mut work = RECT::default();
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut RECT as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let (w, h) = (w.min(work.right - work.left), h.min(work.bottom - work.top));
        let x = work.left + ((work.right - work.left) - w) / 2;
        let y = work.top + ((work.bottom - work.top) - h) / 2;
        let title = to_wide(tr("CustomerFD 控制中心", "CustomerFD Control Center"));
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            w,
            h,
            None,
            None,
            hinst,
            None,
        ) else {
            return;
        };
        refresh_system_theme(Some(hwnd));
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);
        apply_frosted_backdrop(hwnd);
        // Recompute the frame now that the client area covers the caption.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        with_state(app::maybe_auto_check);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CoUninitialize();
    }
}
