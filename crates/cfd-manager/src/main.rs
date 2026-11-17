#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(non_snake_case)]

mod i18n;
mod render_aa;
mod theme;

use cfd_core::config::{
    config_dir, config_path, to_wide, AppConfig, SubFolderEntry, UiLang, CLSID_CUSTOMERFD_STR,
};
use cfd_core::custom_icon::{
    delete_custom_icon, import_image_as_ico, list_custom_icons, load_icon_preview_bgra,
};
use cfd_core::icon::create_app_hicon;
use cfd_core::registry::{
    cleanup_all, find_shell_dll, is_registered, notify_explorer_refresh, register_namespace,
    registered_dll_path, restart_explorer, unregister_namespace,
};
use i18n::{get_lang, set_lang, tr};
use render_aa::{
    bgr_to_rgb, blend_bgra_to_hdc, draw_aa_chevron, draw_aa_circle, draw_aa_pill,
    fill_content_panel_aa, lerp_color_rgb, rgb_to_bgr, stamp_content_panel_alpha,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use theme::{accent_bgr, accent_rgb, refresh_system_theme, theme};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreatePen,
    CreateRoundRectRgn, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect,
    InvalidateRect, LineTo, MoveToEx, RoundRect, SelectClipRgn, SelectObject, SetBkMode,
    SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, DEFAULT_CHARSET,
    DEFAULT_PITCH, DIB_RGB_COLORS, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_PATH_ELLIPSIS, DT_SINGLELINE, DT_VCENTER, FF_DONTCARE, FW_BOLD, FW_NORMAL, FW_SEMIBOLD,
    HDC, HFONT, PAINTSTRUCT, PS_SOLID, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    CloseThemeData, DrawThemeTextEx, OpenThemeData, DTTOPTS, DTT_COMPOSITED, DTT_TEXTCOLOR,
    HTHEME, MARGINS,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, ShellExecuteW, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST,
    FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
    GetSystemMetrics, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassW, SetCursor, SetTimer,
    ShowWindow, TranslateMessage, CS_DBLCLKS, IDC_ARROW, IDC_HAND, IDC_IBEAM, MINMAXINFO, MSG,
    SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_CHAR, WM_CREATE, WM_DESTROY, WM_ERASEBKGND,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_PAINT, WM_SETTINGCHANGE, WM_SIZE, WM_TIMER, WNDCLASSW, WS_CAPTION,
    WS_CLIPCHILDREN, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_THICKFRAME,
};

#[repr(C)]
#[allow(non_snake_case)]
struct OPENFILENAMEW {
    lStructSize: u32,
    hwndOwner: HWND,
    hInstance: HINSTANCE,
    lpstrFilter: PCWSTR,
    lpstrCustomFilter: windows::core::PWSTR,
    nMaxCustFilter: u32,
    nFilterIndex: u32,
    lpstrFile: windows::core::PWSTR,
    nMaxFile: u32,
    lpstrFileTitle: windows::core::PWSTR,
    nMaxFileTitle: u32,
    lpstrInitialDir: PCWSTR,
    lpstrTitle: PCWSTR,
    Flags: u32,
    nFileOffset: u16,
    nFileExtension: u16,
    lpstrDefExt: PCWSTR,
    lCustData: isize,
    lpfnHook: Option<unsafe extern "system" fn() -> usize>,
    lpTemplateName: PCWSTR,
    pvReserved: *mut std::ffi::c_void,
    dwReserved: u32,
    FlagsEx: u32,
}

#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(lpofn: *mut OPENFILENAMEW) -> windows::Win32::Foundation::BOOL;
}

#[link(name = "user32")]
extern "system" {
    fn OpenClipboard(hWndNewOwner: HWND) -> windows::Win32::Foundation::BOOL;
    fn CloseClipboard() -> windows::Win32::Foundation::BOOL;
    fn EmptyClipboard() -> windows::Win32::Foundation::BOOL;
    fn SetClipboardData(
        uFormat: u32,
        hMem: windows::Win32::Foundation::HANDLE,
    ) -> windows::Win32::Foundation::HANDLE;
    fn GetClipboardData(uFormat: u32) -> windows::Win32::Foundation::HANDLE;
    fn GetKeyState(nVirtKey: i32) -> i16;
    fn SetCapture(hWnd: HWND) -> HWND;
    fn ReleaseCapture() -> windows::Win32::Foundation::BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(uFlags: u32, dwBytes: usize) -> windows::Win32::Foundation::HGLOBAL;
    fn GlobalLock(hMem: windows::Win32::Foundation::HGLOBAL) -> *mut std::ffi::c_void;
    fn GlobalUnlock(hMem: windows::Win32::Foundation::HGLOBAL) -> windows::Win32::Foundation::BOOL;
}

const TIMER_ANIM_ID: usize = 1;
const TIMER_CARET_ID: usize = 2;

#[derive(Clone, Copy, PartialEq, Eq)]
enum TabPage {
    Sidebar = 0,
    Subfolders = 1,
    About = 2,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputField {
    RootName,
    RootTargetPath,
    RootIcon,
    SubName,
    SubPath,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CursorKind {
    Arrow,
    Hand,
    Ibeam,
}

#[derive(Clone)]
enum HitAction {
    SwitchTab(TabPage),
    ToggleLang,
    FocusInput(InputField),
    ClearInput(InputField),
    SelectRoot(usize),
    AddRootFolder,
    DeleteRootFolder(usize),
    ToggleShowInSidebar,
    ToggleHideDesktop,
    TogglePreviewExpand,
    ApplyAll,
    Unregister,
    RestartExplorer,
    BrowseIconFile,
    BrowseRootPath,
    ImportCustomIcon,
    SelectIcon(String),
    DeleteCustomIcon(PathBuf),
    BrowseSubPath,
    SaveSubfolder,
    CancelEditSubfolder,
    ToggleSubEnabled(usize),
    MoveSubUp(usize),
    MoveSubDown(usize),
    EditSub(usize),
    OpenSub(usize),
    DeleteSub(usize),
    OpenConfigDir,
    CleanupAll,
}

#[derive(Clone)]
struct HitRegion {
    rect: RECT,
    action: HitAction,
    cursor: CursorKind,
}

#[derive(Clone, Default)]
struct TextEditMeta {
    cursor_char: usize,
    sel_anchor: Option<usize>,
    dragging: bool,
    box_rect: RECT,
}

struct AppState {
    dpi: u32,
    active_tab: TabPage,
    config: AppConfig,
    selected_root_idx: usize,
    registered: bool,
    preview_expanded: bool,
    editing_id: Option<String>,
    sub_name_buf: String,
    sub_path_buf: String,
    focused_input: Option<InputField>,
    input_meta: HashMap<u8, TextEditMeta>,
    caret_visible: bool,
    custom_icons: Vec<PathBuf>,
    icon_previews: HashMap<PathBuf, (u32, Vec<u8>)>,
    status_msg: String,
    status_is_err: bool,
    scroll_y: i32,
    max_scroll: i32,
    hover_idx: Option<usize>,
    hit_regions: Vec<HitRegion>,
    anim_sidebar: f32,
    anim_hide_desktop: f32,
}

thread_local! {
    static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

fn field_key(f: InputField) -> u8 {
    match f {
        InputField::RootName => 0,
        InputField::RootTargetPath => 1,
        InputField::RootIcon => 2,
        InputField::SubName => 3,
        InputField::SubPath => 4,
    }
}

#[inline]
fn s(val: i32, dpi: u32) -> i32 {
    (val * dpi as i32 + 48) / 96
}

fn rect_contains(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

fn char_to_byte_idx(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn get_sel_range(meta: &TextEditMeta, char_len: usize) -> Option<(usize, usize)> {
    let a = meta.sel_anchor?.min(char_len);
    let c = meta.cursor_char.min(char_len);
    if a == c {
        None
    } else {
        Some((a.min(c), a.max(c)))
    }
}

fn delete_selection(text: &mut String, meta: &mut TextEditMeta) -> bool {
    let char_len = text.chars().count();
    if let Some((start, end)) = get_sel_range(meta, char_len) {
        let b0 = char_to_byte_idx(text, start);
        let b1 = char_to_byte_idx(text, end);
        text.replace_range(b0..b1, "");
        meta.cursor_char = start;
        meta.sel_anchor = None;
        true
    } else {
        false
    }
}

unsafe fn create_ui_font(dpi: u32, pt: i32, weight: i32) -> HFONT {
    let height = -((pt * dpi as i32) / 72);
    let face = to_wide("Segoe UI");
    CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        0,
        0,
        CLEARTYPE_QUALITY.0 as u32,
        (FF_DONTCARE.0 | DEFAULT_PITCH.0) as u32,
        PCWSTR(face.as_ptr()),
    )
}

unsafe fn read_clipboard_text(hwnd: HWND) -> Option<String> {
    if !OpenClipboard(hwnd).as_bool() {
        return None;
    }
    let mut out = None;
    let handle = GetClipboardData(13); // CF_UNICODETEXT
    if !handle.is_invalid() {
        let ptr = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0));
        if !ptr.is_null() {
            let wptr = ptr.cast::<u16>();
            let mut len = 0usize;
            while len < 512 && *wptr.add(len) != 0 {
                len += 1;
            }
            out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(wptr, len)));
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
        }
    }
    let _ = CloseClipboard();
    out
}

unsafe fn copy_to_clipboard(hwnd: HWND, text: &str) {
    if !OpenClipboard(hwnd).as_bool() {
        return;
    }
    let _ = EmptyClipboard();
    let wide = to_wide(text);
    let bytes_len = wide.len() * 2;
    let h_mem = GlobalAlloc(0x0042, bytes_len);
    if !h_mem.is_invalid() {
        let ptr = GlobalLock(h_mem);
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), ptr.cast::<u8>(), bytes_len);
            let _ = GlobalUnlock(h_mem);
            let _ = SetClipboardData(13, windows::Win32::Foundation::HANDLE(h_mem.0));
        }
    }
    let _ = CloseClipboard();
}

unsafe fn prompt_open_file(hwnd: HWND, title: &str, filter: &str) -> Option<String> {
    let mut file_buf = [0u16; 1024];
    let filter_w: Vec<u16> = filter.encode_utf16().collect();
    let title_w = to_wide(title);
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: hwnd,
        hInstance: HINSTANCE::default(),
        lpstrFilter: PCWSTR(filter_w.as_ptr()),
        lpstrCustomFilter: windows::core::PWSTR::null(),
        nMaxCustFilter: 0,
        nFilterIndex: 1,
        lpstrFile: windows::core::PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrFileTitle: windows::core::PWSTR::null(),
        nMaxFileTitle: 0,
        lpstrInitialDir: PCWSTR::null(),
        lpstrTitle: PCWSTR(title_w.as_ptr()),
        Flags: 0x00001000 | 0x00000800 | 0x00000008,
        nFileOffset: 0,
        nFileExtension: 0,
        lpstrDefExt: PCWSTR::null(),
        lCustData: 0,
        lpfnHook: None,
        lpTemplateName: PCWSTR::null(),
        pvReserved: std::ptr::null_mut(),
        dwReserved: 0,
        FlagsEx: 0,
    };
    if GetOpenFileNameW(&mut ofn).as_bool() {
        let len = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
        if len > 0 {
            return Some(String::from_utf16_lossy(&file_buf[..len]));
        }
    }
    None
}

unsafe fn draw_composited_text(
    hdc: HDC,
    htheme: HTHEME,
    text: &str,
    rect: &RECT,
    color_bgr: u32,
    font: HFONT,
    flags: u32,
) {
    let old_font = SelectObject(hdc, font);
    let wide = to_wide(text);
    let len = wide.len().saturating_sub(1);
    let mut rc = *rect;
    if !htheme.is_invalid() {
        let opts = DTTOPTS {
            dwSize: std::mem::size_of::<DTTOPTS>() as u32,
            dwFlags: DTT_COMPOSITED | DTT_TEXTCOLOR,
            crText: COLORREF(color_bgr),
            ..Default::default()
        };
        if DrawThemeTextEx(
            htheme,
            hdc,
            0,
            0,
            &wide[..len],
            windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags | DT_NOPREFIX.0),
            &mut rc,
            Some(&opts),
        )
        .is_ok()
        {
            SelectObject(hdc, old_font);
            return;
        }
    }
    SetTextColor(hdc, COLORREF(color_bgr));
    SetBkMode(hdc, TRANSPARENT);
    let mut wide_mut = wide;
    DrawTextW(
        hdc,
        &mut wide_mut[..len],
        &mut rc,
        windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags | DT_NOPREFIX.0),
    );
    SelectObject(hdc, old_font);
}

unsafe fn stamp_opaque_rect(bits: *mut u32, buf_w: i32, buf_h: i32, rc: &RECT) {
    if bits.is_null() || buf_w <= 0 || buf_h <= 0 {
        return;
    }
    let x0 = rc.left.max(0).min(buf_w);
    let x1 = rc.right.max(0).min(buf_w);
    let y0 = rc.top.max(0).min(buf_h);
    let y1 = rc.bottom.max(0).min(buf_h);
    for py in y0..y1 {
        let row = bits.add((py * buf_w) as usize);
        for px in x0..x1 {
            *row.add(px as usize) |= 0xFF00_0000;
        }
    }
}

unsafe fn draw_text_rect(
    hdc: HDC,
    text: &str,
    rect: &RECT,
    color_bgr: u32,
    font: HFONT,
    flags: u32,
) {
    let old_font = SelectObject(hdc, font);
    SetTextColor(hdc, COLORREF(color_bgr));
    SetBkMode(hdc, TRANSPARENT);
    let mut wide = to_wide(text);
    let len = wide.len().saturating_sub(1);
    let mut rc = *rect;
    DrawTextW(
        hdc,
        &mut wide[..len],
        &mut rc,
        windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags | DT_NOPREFIX.0),
    );
    SelectObject(hdc, old_font);
}

unsafe fn measure_text_width(hdc: HDC, text: &str, font: HFONT) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let old_font = SelectObject(hdc, font);
    let mut wide = to_wide(text);
    let len = wide.len().saturating_sub(1);
    let mut rc = RECT::default();
    DrawTextW(
        hdc,
        &mut wide[..len],
        &mut rc,
        DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old_font);
    rc.right - rc.left
}

unsafe fn hit_test_char_index(hdc: HDC, text: &str, rel_x: i32, font: HFONT) -> usize {
    if rel_x <= 0 || text.is_empty() {
        return 0;
    }
    let mut prev_w = 0;
    for (ci, (b, ch)) in text.char_indices().enumerate() {
        let next_b = b + ch.len_utf8();
        let w = measure_text_width(hdc, &text[..next_b], font);
        if rel_x < (prev_w + w) / 2 {
            return ci;
        }
        prev_w = w;
    }
    text.chars().count()
}

unsafe fn draw_round_box(hdc: HDC, rc: &RECT, radius: i32, fill_bgr: u32, border_bgr: u32) {
    let brush = CreateSolidBrush(COLORREF(fill_bgr));
    let pen = CreatePen(PS_SOLID, 1, COLORREF(border_bgr));
    let old_b = SelectObject(hdc, brush);
    let old_p = SelectObject(hdc, pen);
    let _ = RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);
    SelectObject(hdc, old_b);
    SelectObject(hdc, old_p);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
}

unsafe fn draw_custom_input(
    hdc: HDC,
    rc: &RECT,
    dpi: u32,
    field: InputField,
    text: &str,
    placeholder: &str,
    font: HFONT,
    st: &mut AppState,
) {
    let pal = theme();
    let focused = st.focused_input == Some(field);
    let show_clear = focused && !text.is_empty();
    let clear_w = if show_clear { s(24, dpi) } else { 0 };

    let border = if focused { accent_bgr() } else { pal.input_border };
    draw_round_box(hdc, rc, s(6, dpi), pal.input_bg, border);

    let key = field_key(field);
    let meta = st.input_meta.entry(key).or_default().clone();
    if let Some(m) = st.input_meta.get_mut(&key) {
        m.box_rect = *rc;
    }

    let text_left = rc.left + s(10, dpi);
    let text_right = (rc.right - s(8, dpi) - clear_w).max(text_left + s(20, dpi));
    let text_rc = RECT {
        left: text_left,
        top: rc.top,
        right: text_right,
        bottom: rc.bottom,
    };

    // Draw selection highlight if focused & has selection
    let char_len = text.chars().count();
    if focused {
        if let Some((s0, s1)) = get_sel_range(&meta, char_len) {
            let b0 = char_to_byte_idx(text, s0);
            let b1 = char_to_byte_idx(text, s1);
            let x0 = text_left + measure_text_width(hdc, &text[..b0], font);
            let x1 = (text_left + measure_text_width(hdc, &text[..b1], font)).min(text_right);
            if x1 > x0 {
                let sel_rgb = lerp_color_rgb(accent_rgb(), 0x333333, 0.25);
                let sel_brush = CreateSolidBrush(COLORREF(rgb_to_bgr(sel_rgb)));
                let sel_rc = RECT {
                    left: x0,
                    top: rc.top + s(5, dpi),
                    right: x1,
                    bottom: rc.bottom - s(5, dpi),
                };
                FillRect(hdc, &sel_rc, sel_brush);
                let _ = DeleteObject(sel_brush);
            }
        }
    }

    if text.is_empty() && !focused {
        draw_text_rect(
            hdc,
            placeholder,
            &text_rc,
            pal.text_dim,
            font,
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
        );
    } else {
        draw_text_rect(
            hdc,
            text,
            &text_rc,
            pal.text_primary,
            font,
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
        );
    }

    if focused && st.caret_visible {
        let cur_c = meta.cursor_char.min(char_len);
        let cur_b = char_to_byte_idx(text, cur_c);
        let tw = measure_text_width(hdc, &text[..cur_b], font).min(text_right - text_left - 2);
        let cx = text_left + tw + 1;
        let cy0 = rc.top + s(7, dpi);
        let cy1 = rc.bottom - s(7, dpi);
        let pen = CreatePen(PS_SOLID, 1, COLORREF(pal.text_bright));
        let old_p = SelectObject(hdc, pen);
        let _ = MoveToEx(hdc, cx, cy0, None);
        let _ = LineTo(hdc, cx, cy1);
        SelectObject(hdc, old_p);
        let _ = DeleteObject(pen);
    }

    // Clear button (✕) on the right when in input mode and non-empty
    if show_clear {
        let clear_rc = RECT {
            left: rc.right - s(26, dpi),
            top: rc.top + s(4, dpi),
            right: rc.right - s(4, dpi),
            bottom: rc.bottom - s(4, dpi),
        };
        let c_hov = st.hover_idx == Some(st.hit_regions.len());
        if c_hov {
            draw_round_box(hdc, &clear_rc, s(4, dpi), pal.btn_hover_bg, pal.btn_hover_bg);
        }
        draw_text_rect(
            hdc,
            "✕",
            &clear_rc,
            if c_hov { pal.text_bright } else { pal.text_muted },
            font,
            DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );
        st.hit_regions.push(HitRegion {
            rect: clear_rc,
            action: HitAction::ClearInput(field),
            cursor: CursorKind::Hand,
        });
    }

    st.hit_regions.push(HitRegion {
        rect: *rc,
        action: HitAction::FocusInput(field),
        cursor: CursorKind::Ibeam,
    });
}

unsafe fn draw_toggle_switch(hdc: HDC, x: i32, y: i32, dpi: u32, anim: f32) -> RECT {
    let w = s(40, dpi);
    let h = s(20, dpi);
    let pal = theme();
    let off_fill = if pal.dark { 0x2A2626 } else { 0xD0D0D0 };
    let on_fill = accent_rgb();
    let off_border = if pal.dark { 0x605A5A } else { 0x8A8A8A };
    let fill = lerp_color_rgb(off_fill, on_fill, anim);
    let border = lerp_color_rgb(off_border, on_fill, anim);
    draw_aa_pill(hdc, x, y, w, h, fill, 1.0, border, 1.0, 1.2);

    let knob_r = (h as f32) * 0.30;
    let min_cx = x as f32 + (h as f32) * 0.5;
    let max_cx = (x + w) as f32 - (h as f32) * 0.5;
    let cx = (min_cx + (max_cx - min_cx) * anim).round() as i32;
    let cy = y + h / 2;
    let knob_color = if anim > 0.5 || pal.dark {
        0xFFFFFF
    } else {
        0x555555
    };
    draw_aa_circle(hdc, cx, cy, knob_r, knob_color, 1.0, knob_color, 0.0, 0.0);

    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

unsafe fn draw_button(
    hdc: HDC,
    rc: &RECT,
    dpi: u32,
    label: &str,
    font: HFONT,
    primary: bool,
    hovered: bool,
) {
    let pal = theme();
    let radius = s(6, dpi);
    let (fill, border, text_col) = if primary {
        let base_rgb = accent_rgb();
        let bg_rgb = if hovered {
            lerp_color_rgb(base_rgb, 0xFFFFFF, 0.14)
        } else {
            base_rgb
        };
        let bgr = rgb_to_bgr(bg_rgb);
        (bgr, bgr, 0x00FFFFFF)
    } else {
        let bg = if hovered { pal.btn_hover_bg } else { pal.btn_bg };
        let bd = if hovered {
            pal.btn_hover_border
        } else {
            pal.btn_border
        };
        (bg, bd, pal.text_primary)
    };
    draw_round_box(hdc, rc, radius, fill, border);
    draw_text_rect(
        hdc,
        label,
        rc,
        text_col,
        font,
        DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
    );
}

unsafe fn browse_folder_dialog(owner: HWND) -> Option<String> {
    let dialog: IFileOpenDialog =
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let opts = dialog.GetOptions().ok()?;
    dialog
        .SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST)
        .ok()?;
    let title = to_wide(tr("選擇目標資料夾", "Select Target Folder"));
    let _ = dialog.SetTitle(PCWSTR(title.as_ptr()));
    if dialog.Show(owner).is_err() {
        return None;
    }
    let item = dialog.GetResult().ok()?;
    let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = pwstr.to_string().ok();
    CoTaskMemFree(Some(pwstr.0 as *const _));
    path
}

unsafe fn open_path_in_explorer(path: &str) {
    let path_w = to_wide(path);
    let op_w = to_wide("open");
    ShellExecuteW(
        HWND::default(),
        PCWSTR(op_w.as_ptr()),
        PCWSTR(path_w.as_ptr()),
        PCWSTR::null(),
        PCWSTR::null(),
        SW_SHOW,
    );
}

unsafe fn paint_window(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc_screen = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = (rc.right - rc.left).max(1);
    let h = (rc.bottom - rc.top).max(1);

    let mem_dc = CreateCompatibleDC(hdc_screen);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let Ok(bmp) = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) else {
        let _ = DeleteDC(mem_dc);
        let _ = EndPaint(hwnd, &ps);
        return;
    };
    let old_bmp = SelectObject(mem_dc, bmp);
    if !bits_ptr.is_null() {
        std::ptr::write_bytes(bits_ptr as *mut u32, 0, (w as usize) * (h as usize));
    }

    let htheme = OpenThemeData(hwnd, w!("Window"));
    STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        if let Some(st) = borrow.as_mut() {
            render_ui(mem_dc, bits_ptr as *mut u32, htheme, &rc, st);
        }
    });
    if !htheme.is_invalid() {
        let _ = CloseThemeData(htheme);
    }

    let _ = BitBlt(hdc_screen, 0, 0, w, h, mem_dc, 0, 0, SRCCOPY);
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteObject(bmp);
    let _ = DeleteDC(mem_dc);
    let _ = EndPaint(hwnd, &ps);
}

unsafe fn draw_root_selector_row(
    hdc: HDC,
    cx: i32,
    y: i32,
    cw: i32,
    dpi: u32,
    font: HFONT,
    allow_add_del: bool,
    st: &mut AppState,
) -> i32 {
    let pal = theme();
    let mut rx = cx;
    let row_h = s(30, dpi);
    let root_count = st.config.root_folders.len();

    for idx in 0..root_count {
        let name = st.config.root_folders[idx].name.clone();
        let label = format!("📁 {name}");
        let pill_w = (measure_text_width(hdc, &label, font) + s(24, dpi)).min(s(180, dpi));
        if rx + pill_w > cx + cw - s(130, dpi) {
            break;
        }
        let pill_rc = RECT {
            left: rx,
            top: y,
            right: rx + pill_w,
            bottom: y + row_h,
        };
        let selected = st.selected_root_idx == idx;
        let hov = st.hover_idx == Some(st.hit_regions.len());
        let bg = if selected {
            rgb_to_bgr(accent_rgb())
        } else if hov {
            pal.btn_hover_bg
        } else {
            pal.btn_bg
        };
        let bd = if selected {
            rgb_to_bgr(accent_rgb())
        } else if hov {
            pal.btn_hover_border
        } else {
            pal.btn_border
        };
        draw_round_box(hdc, &pill_rc, s(6, dpi), bg, bd);
        draw_text_rect(
            hdc,
            &label,
            &RECT {
                left: pill_rc.left + s(8, dpi),
                top: pill_rc.top,
                right: pill_rc.right - s(8, dpi),
                bottom: pill_rc.bottom,
            },
            if selected { 0x00FFFFFF } else { pal.text_primary },
            font,
            DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
        );
        st.hit_regions.push(HitRegion {
            rect: pill_rc,
            action: HitAction::SelectRoot(idx),
            cursor: CursorKind::Hand,
        });
        rx += pill_w + s(8, dpi);
    }

    if allow_add_del {
        let add_rc = RECT {
            left: rx,
            top: y,
            right: rx + s(118, dpi),
            bottom: y + row_h,
        };
        let a_hov = st.hover_idx == Some(st.hit_regions.len());
        draw_button(hdc, &add_rc, dpi, tr("+ 添加根資料夾", "+ Add Root"), font, false, a_hov);
        st.hit_regions.push(HitRegion {
            rect: add_rc,
            action: HitAction::AddRootFolder,
            cursor: CursorKind::Hand,
        });
        rx = add_rc.right + s(6, dpi);

        if root_count > 1 {
            let del_rc = RECT {
                left: rx,
                top: y,
                right: rx + s(64, dpi),
                bottom: y + row_h,
            };
            let d_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &del_rc, dpi, tr("刪除", "Del"), font, false, d_hov);
            st.hit_regions.push(HitRegion {
                rect: del_rc,
                action: HitAction::DeleteRootFolder(st.selected_root_idx),
                cursor: CursorKind::Hand,
            });
        }
    }

    row_h + s(12, dpi)
}

unsafe fn render_ui(
    hdc: HDC,
    bits: *mut u32,
    htheme: HTHEME,
    rc: &RECT,
    st: &mut AppState,
) {
    let dpi = st.dpi;
    let pal = theme();
    st.hit_regions.clear();
    if st.selected_root_idx >= st.config.root_folders.len() {
        st.selected_root_idx = 0;
    }

    let f_page = create_ui_font(dpi, 18, FW_BOLD.0 as i32);
    let f_head = create_ui_font(dpi, 11, FW_SEMIBOLD.0 as i32);
    let f_body = create_ui_font(dpi, 10, FW_NORMAL.0 as i32);
    let f_small = create_ui_font(dpi, 9, FW_NORMAL.0 as i32);

    let sidebar_w = s(210, dpi);
    let content_radius = s(10, dpi);

    // 1. Content Panel with anti-aliased Win11 Task Manager style rounded top-left corner
    fill_content_panel_aa(
        bits,
        rc.right,
        rc.bottom,
        sidebar_w,
        content_radius,
        pal.win_bg,
        pal.card_border,
    );

    // 2. Left Navigation Rail (Mica Alt translucent background)
    let tabs = [
        (TabPage::Sidebar, tr("側邊欄設定", "Sidebar"), "📁"),
        (TabPage::Subfolders, tr("子資料夾管理", "Subfolders"), "📂"),
        (TabPage::About, tr("關於與清理", "About & Clean"), "ℹ"),
    ];
    for (i, (tab, label, icon)) in tabs.iter().enumerate() {
        let ty = s(20, dpi) + (i as i32) * s(42, dpi);
        let tab_rc = RECT {
            left: s(12, dpi),
            top: ty,
            right: sidebar_w - s(12, dpi),
            bottom: ty + s(36, dpi),
        };
        let active = st.active_tab == *tab;
        let hit_idx = st.hit_regions.len();
        let hovered = st.hover_idx == Some(hit_idx);
        if active || hovered {
            let bg = if active {
                pal.sidebar_hover_bg
            } else {
                pal.card_bg
            };
            draw_round_box(hdc, &tab_rc, s(6, dpi), bg, bg);
            stamp_opaque_rect(bits, rc.right, rc.bottom, &tab_rc);
        }
        if active {
            let pill_rc = RECT {
                left: tab_rc.left + s(2, dpi),
                top: tab_rc.top + s(8, dpi),
                right: tab_rc.left + s(5, dpi),
                bottom: tab_rc.bottom - s(8, dpi),
            };
            draw_round_box(hdc, &pill_rc, s(2, dpi), accent_bgr(), accent_bgr());
            stamp_opaque_rect(bits, rc.right, rc.bottom, &pill_rc);
        }
        let text_rc = RECT {
            left: tab_rc.left + s(16, dpi),
            top: tab_rc.top,
            right: tab_rc.right - s(8, dpi),
            bottom: tab_rc.bottom,
        };
        let full = format!("{icon}   {label}");
        draw_composited_text(
            hdc,
            htheme,
            &full,
            &text_rc,
            if active { pal.text_bright } else { pal.text_secondary },
            f_head,
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );
        st.hit_regions.push(HitRegion {
            rect: tab_rc,
            action: HitAction::SwitchTab(*tab),
            cursor: CursorKind::Hand,
        });
    }

    // Bottom-left Language Switcher
    let lang_rc = RECT {
        left: s(16, dpi),
        top: rc.bottom - s(52, dpi),
        right: sidebar_w - s(16, dpi),
        bottom: rc.bottom - s(18, dpi),
    };
    let lang_idx = st.hit_regions.len();
    let lang_hov = st.hover_idx == Some(lang_idx);
    draw_round_box(
        hdc,
        &lang_rc,
        s(6, dpi),
        if lang_hov { pal.btn_hover_bg } else { pal.btn_bg },
        if lang_hov { pal.btn_hover_border } else { pal.btn_border },
    );
    stamp_opaque_rect(bits, rc.right, rc.bottom, &lang_rc);
    let lang_text = match get_lang() {
        UiLang::ZhTw => "🌐  繁體中文",
        UiLang::En => "🌐  English",
    };
    draw_composited_text(
        hdc,
        htheme,
        lang_text,
        &RECT {
            left: lang_rc.left + s(12, dpi),
            top: lang_rc.top,
            right: lang_rc.right - s(12, dpi),
            bottom: lang_rc.bottom,
        },
        pal.text_primary,
        f_body,
        DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
    );
    st.hit_regions.push(HitRegion {
        rect: lang_rc,
        action: HitAction::ToggleLang,
        cursor: CursorKind::Hand,
    });

    // 3. Clip to content panel rounded top-left corner and draw content
    let content_rgn = CreateRoundRectRgn(
        sidebar_w,
        0,
        rc.right + content_radius + 2,
        rc.bottom + content_radius + 2,
        content_radius * 2,
        content_radius * 2,
    );
    SelectClipRgn(hdc, content_rgn);

    let cx = sidebar_w + s(28, dpi);
    let cw = (rc.right - cx - s(28, dpi)).max(s(300, dpi));

    let (page_icon, page_title) = match st.active_tab {
        TabPage::Sidebar => ("📁", tr("側邊欄設定", "Sidebar Settings")),
        TabPage::Subfolders => ("📂", tr("子資料夾管理", "Subfolder Management")),
        TabPage::About => ("ℹ", tr("關於與移除清理", "About & Uninstall")),
    };
    let header_rc = RECT {
        left: cx,
        top: s(18, dpi),
        right: cx + cw,
        bottom: s(58, dpi),
    };
    draw_text_rect(
        hdc,
        &format!("{page_icon}  {page_title}"),
        &header_rc,
        pal.text_bright,
        f_page,
        DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
    );

    let mut y_cursor = s(72, dpi) - st.scroll_y;
    let rad = s(8, dpi);

    match st.active_tab {
        TabPage::Sidebar => {
            // Card 1: Explorer Integration + Registration Status on header right
            let c1_h = s(180, dpi);
            let c1 = RECT {
                left: cx,
                top: y_cursor,
                right: cx + cw,
                bottom: y_cursor + c1_h,
            };
            draw_round_box(hdc, &c1, rad, pal.card_bg, pal.card_border);
            draw_text_rect(
                hdc,
                tr("檔案總管左側邊欄整合", "Windows 11 Explorer Sidebar Integration"),
                &RECT {
                    left: c1.left + s(18, dpi),
                    top: c1.top + s(14, dpi),
                    right: c1.right - s(160, dpi),
                    bottom: c1.top + s(38, dpi),
                },
                pal.text_bright,
                f_head,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );

            let dot_rgb = if st.registered {
                if pal.dark { 0x5FCB6C } else { 0x107C10 }
            } else {
                if pal.dark { 0xFFB833 } else { 0xD47800 }
            };
            let status_txt = if st.registered {
                tr("已啟用", "Registered")
            } else {
                tr("未註冊", "Not Registered")
            };
            let status_col = if st.registered { pal.status_ok } else { pal.status_warn };
            let badge_w = measure_text_width(hdc, status_txt, f_small) + s(24, dpi);
            let dot_x = c1.right - s(18, dpi) - badge_w + s(8, dpi);
            let dot_y = c1.top + s(26, dpi);
            draw_aa_circle(hdc, dot_x, dot_y, s(4, dpi) as f32, dot_rgb, 1.0, dot_rgb, 0.0, 0.0);
            draw_text_rect(
                hdc,
                status_txt,
                &RECT {
                    left: dot_x + s(10, dpi),
                    top: c1.top + s(14, dpi),
                    right: c1.right - s(18, dpi),
                    bottom: c1.top + s(38, dpi),
                },
                status_col,
                f_small,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );

            let t1_y = c1.top + s(48, dpi);
            draw_text_rect(
                hdc,
                tr("在檔案總管左側邊欄顯示自訂資料夾", "Show custom folder in Explorer left sidebar"),
                &RECT {
                    left: c1.left + s(18, dpi),
                    top: t1_y,
                    right: c1.right - s(80, dpi),
                    bottom: t1_y + s(24, dpi),
                },
                pal.text_primary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            let sw1_rc = draw_toggle_switch(hdc, c1.right - s(60, dpi), t1_y + s(2, dpi), dpi, st.anim_sidebar);
            st.hit_regions.push(HitRegion {
                rect: sw1_rc,
                action: HitAction::ToggleShowInSidebar,
                cursor: CursorKind::Hand,
            });

            let t2_y = c1.top + s(84, dpi);
            draw_text_rect(
                hdc,
                tr("從桌面圖示隱藏（僅顯示於側邊欄）", "Hide from Desktop icons (sidebar only)"),
                &RECT {
                    left: c1.left + s(18, dpi),
                    top: t2_y,
                    right: c1.right - s(80, dpi),
                    bottom: t2_y + s(24, dpi),
                },
                pal.text_primary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            let sw2_rc = draw_toggle_switch(hdc, c1.right - s(60, dpi), t2_y + s(2, dpi), dpi, st.anim_hide_desktop);
            st.hit_regions.push(HitRegion {
                rect: sw2_rc,
                action: HitAction::ToggleHideDesktop,
                cursor: CursorKind::Hand,
            });

            let btn_y = c1.top + s(126, dpi);
            let btn_h = s(34, dpi);
            let b1 = RECT {
                left: c1.left + s(18, dpi),
                top: btn_y,
                right: c1.left + s(196, dpi),
                bottom: btn_y + btn_h,
            };
            let b1_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &b1, dpi, tr("套用並更新側邊欄", "Apply & Refresh"), f_head, true, b1_hov);
            st.hit_regions.push(HitRegion {
                rect: b1,
                action: HitAction::ApplyAll,
                cursor: CursorKind::Hand,
            });

            let b2 = RECT {
                left: b1.right + s(12, dpi),
                top: btn_y,
                right: b1.right + s(130, dpi),
                bottom: btn_y + btn_h,
            };
            let b2_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &b2, dpi, tr("移除註冊", "Unregister"), f_body, false, b2_hov);
            st.hit_regions.push(HitRegion {
                rect: b2,
                action: HitAction::Unregister,
                cursor: CursorKind::Hand,
            });

            let b3 = RECT {
                left: b2.right + s(12, dpi),
                top: btn_y,
                right: b2.right + s(156, dpi),
                bottom: btn_y + btn_h,
            };
            let b3_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &b3, dpi, tr("重啟檔案總管", "Restart Explorer"), f_body, false, b3_hov);
            st.hit_regions.push(HitRegion {
                rect: b3,
                action: HitAction::RestartExplorer,
                cursor: CursorKind::Hand,
            });

            y_cursor += c1_h + s(14, dpi);

            // Root Folder Selector + Add Root Folder row
            y_cursor += draw_root_selector_row(hdc, cx, y_cursor, cw, dpi, f_small, true, st);

            // Card 2: Selected Root Folder Name, Target Path (UNC/Local), Icon Resource & Saved Icon Previews + Sidebar Preview
            let custom_rows = st.custom_icons.len().min(6);
            let c2_h = s(292, dpi) + (custom_rows as i32) * s(36, dpi) + if custom_rows > 0 { s(28, dpi) } else { 0 };
            let c2 = RECT {
                left: cx,
                top: y_cursor,
                right: cx + cw,
                bottom: y_cursor + c2_h,
            };
            draw_round_box(hdc, &c2, rad, pal.card_bg, pal.card_border);

            let input_x = c2.left + s(18, dpi);
            let input_w = (cw - s(276, dpi)).max(s(220, dpi));
            let sel_root = st.config.root_folders[st.selected_root_idx].clone();

            draw_text_rect(
                hdc,
                tr("根資料夾名稱（顯示於側邊欄）", "Root Folder Name (shown in sidebar)"),
                &RECT {
                    left: input_x,
                    top: c2.top + s(16, dpi),
                    right: input_x + input_w,
                    bottom: c2.top + s(38, dpi),
                },
                pal.text_secondary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            let box1 = RECT {
                left: input_x,
                top: c2.top + s(42, dpi),
                right: input_x + input_w,
                bottom: c2.top + s(74, dpi),
            };
            draw_custom_input(
                hdc,
                &box1,
                dpi,
                InputField::RootName,
                &sel_root.name,
                tr("輸入資料夾名稱...", "Enter folder name..."),
                f_body,
                st,
            );

            // Root Target Path (optional, supports Local & UNC \\NAS\Share)
            draw_text_rect(
                hdc,
                tr(
                    "根資料夾連線路徑（選填，支援本機或 UNC 網路路徑 \\\\NAS\\Share）",
                    "Root Target Path (optional, supports Local or UNC \\\\NAS\\Share)",
                ),
                &RECT {
                    left: input_x,
                    top: c2.top + s(84, dpi),
                    right: input_x + input_w,
                    bottom: c2.top + s(106, dpi),
                },
                pal.text_secondary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
            );
            let root_br_w = s(76, dpi);
            let root_path_w = (input_w - root_br_w - s(6, dpi)).max(s(140, dpi));
            let box_rpath = RECT {
                left: input_x,
                top: c2.top + s(110, dpi),
                right: input_x + root_path_w,
                bottom: c2.top + s(142, dpi),
            };
            draw_custom_input(
                hdc,
                &box_rpath,
                dpi,
                InputField::RootTargetPath,
                &sel_root.target_path,
                tr("選填：D:\\Workspace 或 \\\\NAS\\Share", "Optional: D:\\Data or \\\\NAS\\Share"),
                f_body,
                st,
            );
            let br_rpath_rc = RECT {
                left: box_rpath.right + s(6, dpi),
                top: box_rpath.top,
                right: input_x + input_w,
                bottom: box_rpath.bottom,
            };
            let br_rp_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &br_rpath_rc, dpi, tr("瀏覽...", "Browse..."), f_small, false, br_rp_hov);
            st.hit_regions.push(HitRegion {
                rect: br_rpath_rc,
                action: HitAction::BrowseRootPath,
                cursor: CursorKind::Hand,
            });

            draw_text_rect(
                hdc,
                tr("圖示資源（可瀏覽現有 ICO/DLL 或自訂圖片轉換）", "Icon Resource (Browse ICO/DLL or Convert Image)"),
                &RECT {
                    left: input_x,
                    top: c2.top + s(152, dpi),
                    right: input_x + input_w,
                    bottom: c2.top + s(174, dpi),
                },
                pal.text_secondary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            let icon_btn_w = s(76, dpi);
            let custom_btn_w = s(92, dpi);
            let icon_input_w = (input_w - icon_btn_w - custom_btn_w - s(12, dpi)).max(s(120, dpi));
            let box2 = RECT {
                left: input_x,
                top: c2.top + s(178, dpi),
                right: input_x + icon_input_w,
                bottom: c2.top + s(210, dpi),
            };
            draw_custom_input(
                hdc,
                &box2,
                dpi,
                InputField::RootIcon,
                &sel_root.icon,
                "shell32.dll,-4",
                f_body,
                st,
            );

            let br_icon_rc = RECT {
                left: box2.right + s(6, dpi),
                top: box2.top,
                right: box2.right + s(6, dpi) + icon_btn_w,
                bottom: box2.bottom,
            };
            let br_i_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &br_icon_rc, dpi, tr("瀏覽...", "Browse..."), f_small, false, br_i_hov);
            st.hit_regions.push(HitRegion {
                rect: br_icon_rc,
                action: HitAction::BrowseIconFile,
                cursor: CursorKind::Hand,
            });

            let imp_icon_rc = RECT {
                left: br_icon_rc.right + s(6, dpi),
                top: box2.top,
                right: input_x + input_w,
                bottom: box2.bottom,
            };
            let imp_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &imp_icon_rc, dpi, tr("+ 自訂圖示", "+ Custom"), f_small, true, imp_hov);
            st.hit_regions.push(HitRegion {
                rect: imp_icon_rc,
                action: HitAction::ImportCustomIcon,
                cursor: CursorKind::Hand,
            });

            let def_icon_rc = RECT {
                left: input_x,
                top: c2.top + s(220, dpi),
                right: input_x + s(146, dpi),
                bottom: c2.top + s(248, dpi),
            };
            let def_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(
                hdc,
                &def_icon_rc,
                dpi,
                tr("重設為預設資料夾圖示", "Default Folder Icon"),
                f_small,
                false,
                def_hov,
            );
            st.hit_regions.push(HitRegion {
                rect: def_icon_rc,
                action: HitAction::SelectIcon("shell32.dll,-4".to_string()),
                cursor: CursorKind::Hand,
            });

            // Managed Custom Icons List WITH ACTUAL 20x20 ICON PREVIEWS
            if !st.custom_icons.is_empty() {
                let list_top = c2.top + s(260, dpi);
                draw_text_rect(
                    hdc,
                    tr("已儲存的自訂圖示（點擊選用，✕ 刪除）", "Saved Custom Icons (click to apply, ✕ to delete)"),
                    &RECT {
                        left: input_x,
                        top: list_top,
                        right: input_x + input_w,
                        bottom: list_top + s(22, dpi),
                    },
                    pal.text_muted,
                    f_small,
                    DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
                );
                let mut iy = list_top + s(26, dpi);
                let ico_sz = s(20, dpi) as u32;
                let icons_snapshot = st.custom_icons.clone();
                for icon_path in icons_snapshot.iter().take(6) {
                    let path_str = icon_path.display().to_string();
                    let is_sel = sel_root.icon.eq_ignore_ascii_case(&path_str);
                    let fname = icon_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("icon.ico");
                    let item_rc = RECT {
                        left: input_x,
                        top: iy,
                        right: input_x + input_w - s(36, dpi),
                        bottom: iy + s(30, dpi),
                    };
                    let i_hov = st.hover_idx == Some(st.hit_regions.len());
                    let bg = if is_sel {
                        pal.sidebar_hover_bg
                    } else if i_hov {
                        pal.btn_hover_bg
                    } else {
                        pal.sub_card_bg
                    };
                    let bd = if is_sel { accent_bgr() } else { pal.sub_card_border };
                    draw_round_box(hdc, &item_rc, s(5, dpi), bg, bd);

                    // Render cached 20x20 icon preview
                    let preview_entry = st.icon_previews.get(icon_path).cloned();
                    let bgra_opt = match preview_entry {
                        Some((sz, buf)) if sz == ico_sz => Some(buf),
                        _ => {
                            if let Some(buf) = load_icon_preview_bgra(icon_path, ico_sz) {
                                st.icon_previews.insert(icon_path.clone(), (ico_sz, buf.clone()));
                                Some(buf)
                            } else {
                                None
                            }
                        }
                    };
                    if let Some(bgra) = bgra_opt {
                        blend_bgra_to_hdc(
                            hdc,
                            item_rc.left + s(8, dpi),
                            item_rc.top + s(5, dpi),
                            ico_sz as i32,
                            ico_sz as i32,
                            &bgra,
                        );
                    }

                    draw_text_rect(
                        hdc,
                        fname,
                        &RECT {
                            left: item_rc.left + s(34, dpi),
                            top: item_rc.top,
                            right: item_rc.right - s(10, dpi),
                            bottom: item_rc.bottom,
                        },
                        if is_sel { pal.text_bright } else { pal.text_secondary },
                        f_small,
                        DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
                    );
                    st.hit_regions.push(HitRegion {
                        rect: item_rc,
                        action: HitAction::SelectIcon(path_str),
                        cursor: CursorKind::Hand,
                    });

                    let del_rc = RECT {
                        left: item_rc.right + s(6, dpi),
                        top: iy,
                        right: input_x + input_w,
                        bottom: iy + s(30, dpi),
                    };
                    let d_hov = st.hover_idx == Some(st.hit_regions.len());
                    draw_button(hdc, &del_rc, dpi, "✕", f_small, false, d_hov);
                    st.hit_regions.push(HitRegion {
                        rect: del_rc,
                        action: HitAction::DeleteCustomIcon(icon_path.clone()),
                        cursor: CursorKind::Hand,
                    });
                    iy += s(36, dpi);
                }
            }

            // Live Sidebar Preview Box on the right (with Anti-Aliased drawn chevrons!)
            let prev_rc = RECT {
                left: input_x + input_w + s(20, dpi),
                top: c2.top + s(18, dpi),
                right: c2.right - s(18, dpi),
                bottom: c2.top + s(208, dpi),
            };
            draw_round_box(hdc, &prev_rc, s(6, dpi), pal.sub_card_bg, pal.sub_card_border);
            draw_text_rect(
                hdc,
                tr("側邊欄預覽（點擊展開/摺疊）", "Sidebar Preview (click to expand)"),
                &RECT {
                    left: prev_rc.left + s(12, dpi),
                    top: prev_rc.top + s(8, dpi),
                    right: prev_rc.right - s(12, dpi),
                    bottom: prev_rc.top + s(28, dpi),
                },
                pal.text_muted,
                f_small,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );

            let root_preview_rc = RECT {
                left: prev_rc.left + s(10, dpi),
                top: prev_rc.top + s(34, dpi),
                right: prev_rc.right - s(10, dpi),
                bottom: prev_rc.top + s(60, dpi),
            };
            let chevron_rgb = bgr_to_rgb(pal.text_secondary);
            draw_aa_chevron(
                hdc,
                root_preview_rc.left + s(8, dpi),
                (root_preview_rc.top + root_preview_rc.bottom) / 2,
                s(4, dpi) as f32,
                st.preview_expanded,
                chevron_rgb,
                1.0,
                1.5,
            );
            let root_name_disp = if sel_root.name.trim().is_empty() {
                "自訂資料夾"
            } else {
                sel_root.name.trim()
            };
            draw_text_rect(
                hdc,
                &format!("📁  {root_name_disp}"),
                &RECT {
                    left: root_preview_rc.left + s(20, dpi),
                    top: root_preview_rc.top,
                    right: root_preview_rc.right,
                    bottom: root_preview_rc.bottom,
                },
                pal.text_bright,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
            );
            st.hit_regions.push(HitRegion {
                rect: root_preview_rc,
                action: HitAction::TogglePreviewExpand,
                cursor: CursorKind::Hand,
            });

            if st.preview_expanded {
                let mut py = prev_rc.top + s(64, dpi);
                for sub in st
                    .config
                    .subfolders
                    .iter()
                    .filter(|e| e.enabled && e.root_id == sel_root.id)
                    .take(5)
                {
                    let sub_rc = RECT {
                        left: prev_rc.left + s(24, dpi),
                        top: py,
                        right: prev_rc.right - s(12, dpi),
                        bottom: py + s(22, dpi),
                    };
                    draw_aa_chevron(
                        hdc,
                        sub_rc.left + s(6, dpi),
                        (sub_rc.top + sub_rc.bottom) / 2,
                        s(3, dpi) as f32,
                        false,
                        chevron_rgb,
                        0.85,
                        1.3,
                    );
                    draw_text_rect(
                        hdc,
                        &format!("📂  {}", sub.name),
                        &RECT {
                            left: sub_rc.left + s(16, dpi),
                            top: sub_rc.top,
                            right: sub_rc.right,
                            bottom: sub_rc.bottom,
                        },
                        pal.text_secondary,
                        f_small,
                        DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
                    );
                    py += s(24, dpi);
                }
            }

            y_cursor += c2_h + s(16, dpi);
            st.max_scroll = (y_cursor + st.scroll_y - rc.bottom + s(44, dpi)).max(0);
        }
        TabPage::Subfolders => {
            // Root folder selector for Subfolder management
            y_cursor += draw_root_selector_row(hdc, cx, y_cursor, cw, dpi, f_small, false, st);
            let sel_root_id = st.config.root_folders[st.selected_root_idx].id.clone();

            // Card 1: Add / Edit Subfolder
            let c1_h = s(142, dpi);
            let c1 = RECT {
                left: cx,
                top: y_cursor,
                right: cx + cw,
                bottom: y_cursor + c1_h,
            };
            draw_round_box(hdc, &c1, rad, pal.card_bg, pal.card_border);

            let half_w = (cw - s(48, dpi)) / 2;
            let name_x = c1.left + s(18, dpi);
            let path_x = name_x + half_w + s(12, dpi);
            let browse_w = s(92, dpi);
            let path_w = (c1.right - s(18, dpi) - browse_w - s(8, dpi) - path_x).max(s(140, dpi));

            draw_text_rect(
                hdc,
                tr("子資料夾名稱", "Subfolder Name"),
                &RECT {
                    left: name_x,
                    top: c1.top + s(16, dpi),
                    right: name_x + half_w,
                    bottom: c1.top + s(38, dpi),
                },
                pal.text_secondary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            draw_text_rect(
                hdc,
                tr("對應實體路徑", "Mapped Target Path"),
                &RECT {
                    left: path_x,
                    top: c1.top + s(16, dpi),
                    right: path_x + path_w,
                    bottom: c1.top + s(38, dpi),
                },
                pal.text_secondary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );

            let input_y = c1.top + s(44, dpi);
            let name_box = RECT {
                left: name_x,
                top: input_y,
                right: name_x + half_w,
                bottom: input_y + s(32, dpi),
            };
            let sub_name_val = st.sub_name_buf.clone();
            draw_custom_input(
                hdc,
                &name_box,
                dpi,
                InputField::SubName,
                &sub_name_val,
                tr("例如：專案開發", "e.g. Projects"),
                f_body,
                st,
            );

            let path_box = RECT {
                left: path_x,
                top: input_y,
                right: path_x + path_w,
                bottom: input_y + s(32, dpi),
            };
            let sub_path_val = st.sub_path_buf.clone();
            draw_custom_input(
                hdc,
                &path_box,
                dpi,
                InputField::SubPath,
                &sub_path_val,
                tr("例如：D:\\Workspace 或 \\\\NAS\\Share", "e.g. D:\\Workspace or \\\\NAS\\Share"),
                f_body,
                st,
            );

            let browse_rc = RECT {
                left: path_x + path_w + s(8, dpi),
                top: input_y,
                right: c1.right - s(18, dpi),
                bottom: input_y + s(32, dpi),
            };
            let br_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &browse_rc, dpi, tr("瀏覽...", "Browse..."), f_body, false, br_hov);
            st.hit_regions.push(HitRegion {
                rect: browse_rc,
                action: HitAction::BrowseSubPath,
                cursor: CursorKind::Hand,
            });

            let add_y = c1.top + s(90, dpi);
            let add_rc = RECT {
                left: name_x,
                top: add_y,
                right: name_x + s(160, dpi),
                bottom: add_y + s(34, dpi),
            };
            let add_hov = st.hover_idx == Some(st.hit_regions.len());
            let add_lbl = if st.editing_id.is_some() {
                tr("儲存變更", "Save Changes")
            } else {
                tr("+ 新增子資料夾", "+ Add Subfolder")
            };
            draw_button(hdc, &add_rc, dpi, add_lbl, f_head, true, add_hov);
            st.hit_regions.push(HitRegion {
                rect: add_rc,
                action: HitAction::SaveSubfolder,
                cursor: CursorKind::Hand,
            });

            if st.editing_id.is_some() {
                let cancel_rc = RECT {
                    left: add_rc.right + s(10, dpi),
                    top: add_y,
                    right: add_rc.right + s(106, dpi),
                    bottom: add_y + s(34, dpi),
                };
                let c_hov = st.hover_idx == Some(st.hit_regions.len());
                draw_button(hdc, &cancel_rc, dpi, tr("取消", "Cancel"), f_body, false, c_hov);
                st.hit_regions.push(HitRegion {
                    rect: cancel_rc,
                    action: HitAction::CancelEditSubfolder,
                    cursor: CursorKind::Hand,
                });
            }

            y_cursor += c1_h + s(14, dpi);

            let total_subs = st.config.subfolders.len();
            for i in 0..total_subs {
                if st.config.subfolders[i].root_id != sel_root_id {
                    continue;
                }
                let sub = st.config.subfolders[i].clone();
                let row_h = s(68, dpi);
                let row_rc = RECT {
                    left: cx,
                    top: y_cursor,
                    right: cx + cw,
                    bottom: y_cursor + row_h,
                };
                draw_round_box(hdc, &row_rc, rad, pal.card_bg, pal.card_border);

                let sw_rc = draw_toggle_switch(
                    hdc,
                    row_rc.left + s(16, dpi),
                    row_rc.top + s(24, dpi),
                    dpi,
                    if sub.enabled { 1.0 } else { 0.0 },
                );
                st.hit_regions.push(HitRegion {
                    rect: sw_rc,
                    action: HitAction::ToggleSubEnabled(i),
                    cursor: CursorKind::Hand,
                });

                let text_left = row_rc.left + s(70, dpi);
                let btns_w = s(246, dpi);
                let text_right = (row_rc.right - btns_w - s(16, dpi)).max(text_left + s(80, dpi));
                draw_text_rect(
                    hdc,
                    &format!("📂  {}", sub.name),
                    &RECT {
                        left: text_left,
                        top: row_rc.top + s(10, dpi),
                        right: text_right,
                        bottom: row_rc.top + s(34, dpi),
                    },
                    if sub.enabled { pal.text_bright } else { pal.text_muted },
                    f_head,
                    DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
                );

                let t_trimmed = sub.target_path.trim();
                let is_unc = t_trimmed.starts_with("\\\\");
                let exists = !t_trimmed.is_empty() && (is_unc || Path::new(t_trimmed).exists());
                let path_col = if exists { pal.text_muted } else { pal.status_err };
                let path_disp = if is_unc {
                    format!("{t_trimmed}  (UNC)")
                } else if exists {
                    sub.target_path.clone()
                } else {
                    format!("{} ({})", sub.target_path, tr("路徑不存在", "Missing"))
                };
                draw_text_rect(
                    hdc,
                    &path_disp,
                    &RECT {
                        left: text_left,
                        top: row_rc.top + s(34, dpi),
                        right: text_right,
                        bottom: row_rc.top + s(56, dpi),
                    },
                    path_col,
                    f_small,
                    DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_PATH_ELLIPSIS.0,
                );

                let by = row_rc.top + s(19, dpi);
                let bh = s(30, dpi);
                let mut bx = row_rc.right - s(14, dpi);

                let del_rc = RECT { left: bx - s(48, dpi), top: by, right: bx, bottom: by + bh };
                let h_del = st.hover_idx == Some(st.hit_regions.len());
                draw_button(hdc, &del_rc, dpi, tr("刪除", "Del"), f_small, false, h_del);
                st.hit_regions.push(HitRegion { rect: del_rc, action: HitAction::DeleteSub(i), cursor: CursorKind::Hand });
                bx = del_rc.left - s(6, dpi);

                let open_rc = RECT { left: bx - s(48, dpi), top: by, right: bx, bottom: by + bh };
                let h_open = st.hover_idx == Some(st.hit_regions.len());
                draw_button(hdc, &open_rc, dpi, tr("開啟", "Open"), f_small, false, h_open);
                st.hit_regions.push(HitRegion { rect: open_rc, action: HitAction::OpenSub(i), cursor: CursorKind::Hand });
                bx = open_rc.left - s(6, dpi);

                let edit_rc = RECT { left: bx - s(48, dpi), top: by, right: bx, bottom: by + bh };
                let h_edit = st.hover_idx == Some(st.hit_regions.len());
                draw_button(hdc, &edit_rc, dpi, tr("編輯", "Edit"), f_small, false, h_edit);
                st.hit_regions.push(HitRegion { rect: edit_rc, action: HitAction::EditSub(i), cursor: CursorKind::Hand });
                bx = edit_rc.left - s(6, dpi);

                let dn_rc = RECT { left: bx - s(28, dpi), top: by, right: bx, bottom: by + bh };
                let h_dn = st.hover_idx == Some(st.hit_regions.len());
                draw_button(hdc, &dn_rc, dpi, "▼", f_small, false, h_dn);
                st.hit_regions.push(HitRegion { rect: dn_rc, action: HitAction::MoveSubDown(i), cursor: CursorKind::Hand });
                bx = dn_rc.left - s(4, dpi);

                let up_rc = RECT { left: bx - s(28, dpi), top: by, right: bx, bottom: by + bh };
                let h_up = st.hover_idx == Some(st.hit_regions.len());
                draw_button(hdc, &up_rc, dpi, "▲", f_small, false, h_up);
                st.hit_regions.push(HitRegion { rect: up_rc, action: HitAction::MoveSubUp(i), cursor: CursorKind::Hand });

                y_cursor += row_h + s(10, dpi);
            }

            let apply_rc = RECT {
                left: cx,
                top: y_cursor + s(4, dpi),
                right: cx + s(210, dpi),
                bottom: y_cursor + s(38, dpi),
            };
            let a_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &apply_rc, dpi, tr("套用並更新檔案總管", "Apply & Notify Explorer"), f_head, true, a_hov);
            st.hit_regions.push(HitRegion {
                rect: apply_rc,
                action: HitAction::ApplyAll,
                cursor: CursorKind::Hand,
            });
            y_cursor += s(52, dpi);
            st.max_scroll = (y_cursor + st.scroll_y - rc.bottom + s(44, dpi)).max(0);
        }
        TabPage::About => {
            let c1_h = s(170, dpi);
            let c1 = RECT {
                left: cx,
                top: y_cursor,
                right: cx + cw,
                bottom: y_cursor + c1_h,
            };
            draw_round_box(hdc, &c1, rad, pal.card_bg, pal.card_border);
            draw_text_rect(
                hdc,
                "CustomerFD — Windows 11 Explorer Custom Sidebar Folder",
                &RECT {
                    left: c1.left + s(18, dpi),
                    top: c1.top + s(18, dpi),
                    right: c1.right - s(18, dpi),
                    bottom: c1.top + s(44, dpi),
                },
                pal.text_bright,
                f_head,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            draw_text_rect(
                hdc,
                tr(
                    "採用 Rust 開發的 Windows 11 檔案總管 Shell Namespace Extension，支援多個自訂側邊欄根資料夾、展開／摺疊與子資料夾路徑對應。",
                    "Rust-based Windows 11 Explorer Shell Namespace Extension supporting multiple custom expandable sidebar folders.",
                ),
                &RECT {
                    left: c1.left + s(18, dpi),
                    top: c1.top + s(48, dpi),
                    right: c1.right - s(18, dpi),
                    bottom: c1.top + s(90, dpi),
                },
                pal.text_secondary,
                f_body,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
            );

            let dll_str = registered_dll_path()
                .or_else(|| find_shell_dll().map(|p| p.display().to_string()))
                .unwrap_or_else(|| tr("未找到 cfd_shell.dll", "cfd_shell.dll not found").to_string());
            draw_text_rect(
                hdc,
                &format!("Base CLSID: {CLSID_CUSTOMERFD_STR}   |   DLL: {dll_str}"),
                &RECT {
                    left: c1.left + s(18, dpi),
                    top: c1.top + s(90, dpi),
                    right: c1.right - s(18, dpi),
                    bottom: c1.top + s(114, dpi),
                },
                pal.text_muted,
                f_small,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_PATH_ELLIPSIS.0,
            );

            let open_cfg_rc = RECT {
                left: c1.left + s(18, dpi),
                top: c1.top + s(122, dpi),
                right: c1.left + s(180, dpi),
                bottom: c1.top + s(154, dpi),
            };
            let o_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(hdc, &open_cfg_rc, dpi, tr("開啟設定檔目錄", "Open Config Folder"), f_body, false, o_hov);
            st.hit_regions.push(HitRegion {
                rect: open_cfg_rc,
                action: HitAction::OpenConfigDir,
                cursor: CursorKind::Hand,
            });

            draw_text_rect(
                hdc,
                &config_path().display().to_string(),
                &RECT {
                    left: open_cfg_rc.right + s(12, dpi),
                    top: open_cfg_rc.top,
                    right: c1.right - s(18, dpi),
                    bottom: open_cfg_rc.bottom,
                },
                pal.text_muted,
                f_small,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_PATH_ELLIPSIS.0,
            );

            y_cursor += c1_h + s(14, dpi);

            // Card 2: Uninstall & Cleanup Tool
            let c2_h = s(124, dpi);
            let c2 = RECT {
                left: cx,
                top: y_cursor,
                right: cx + cw,
                bottom: y_cursor + c2_h,
            };
            draw_round_box(hdc, &c2, rad, pal.card_bg, pal.card_border);
            draw_text_rect(
                hdc,
                tr("移除與殘存清理工具", "Uninstall & Cleanup Tool"),
                &RECT {
                    left: c2.left + s(18, dpi),
                    top: c2.top + s(16, dpi),
                    right: c2.right - s(18, dpi),
                    bottom: c2.top + s(40, dpi),
                },
                pal.text_bright,
                f_head,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
            );
            draw_text_rect(
                hdc,
                tr(
                    "完整移除所有 HKCU 登錄檔項目、自訂圖示快取與 config.json 設定檔（亦可執行 dist\\uninstall.ps1）。",
                    "Removes all HKCU registry entries, custom icons, and config.json (or run dist\\uninstall.ps1).",
                ),
                &RECT {
                    left: c2.left + s(18, dpi),
                    top: c2.top + s(42, dpi),
                    right: c2.right - s(18, dpi),
                    bottom: c2.top + s(68, dpi),
                },
                pal.text_muted,
                f_small,
                DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
            );

            let clean_rc = RECT {
                left: c2.left + s(18, dpi),
                top: c2.top + s(76, dpi),
                right: c2.left + s(228, dpi),
                bottom: c2.top + s(108, dpi),
            };
            let cl_hov = st.hover_idx == Some(st.hit_regions.len());
            draw_button(
                hdc,
                &clean_rc,
                dpi,
                tr("完全清理所有設定與註冊", "Clean All Settings & Registry"),
                f_body,
                false,
                cl_hov,
            );
            st.hit_regions.push(HitRegion {
                rect: clean_rc,
                action: HitAction::CleanupAll,
                cursor: CursorKind::Hand,
            });

            st.max_scroll = 0;
        }
    }

    if !st.status_msg.is_empty() {
        let toast_rc = RECT {
            left: cx,
            top: rc.bottom - s(36, dpi),
            right: rc.right - s(28, dpi),
            bottom: rc.bottom - s(10, dpi),
        };
        draw_text_rect(
            hdc,
            &st.status_msg,
            &toast_rc,
            if st.status_is_err { pal.status_err } else { pal.status_ok },
            f_body,
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
        );
    }

    SelectClipRgn(hdc, None);
    let _ = DeleteObject(content_rgn);

    stamp_content_panel_alpha(bits, rc.right, rc.bottom, sidebar_w, content_radius);

    let _ = DeleteObject(f_page);
    let _ = DeleteObject(f_head);
    let _ = DeleteObject(f_body);
    let _ = DeleteObject(f_small);
}

fn input_buf_mut(st: &mut AppState, field: InputField) -> &mut String {
    let idx = st.selected_root_idx.min(st.config.root_folders.len().saturating_sub(1));
    match field {
        InputField::RootName => &mut st.config.root_folders[idx].name,
        InputField::RootTargetPath => &mut st.config.root_folders[idx].target_path,
        InputField::RootIcon => &mut st.config.root_folders[idx].icon,
        InputField::SubName => &mut st.sub_name_buf,
        InputField::SubPath => &mut st.sub_path_buf,
    }
}

fn active_input_parts(st: &mut AppState) -> Option<(&mut String, TextEditMeta, u8)> {
    let f = st.focused_input?;
    let k = field_key(f);
    let meta = st.input_meta.get(&k).cloned().unwrap_or_default();
    let buf = input_buf_mut(st, f);
    Some((buf, meta, k))
}

unsafe fn handle_action(hwnd: HWND, action: HitAction, click_x: i32) {
    STATE.with(|cell| {
        let mut borrow = cell.borrow_mut();
        let Some(st) = borrow.as_mut() else {
            return;
        };
        if !matches!(action, HitAction::FocusInput(_) | HitAction::ClearInput(_)) {
            st.focused_input = None;
            let _ = KillTimer(hwnd, TIMER_CARET_ID);
        }

        match action {
            HitAction::SwitchTab(t) => {
                st.active_tab = t;
                st.scroll_y = 0;
            }
            HitAction::ToggleLang => {
                let next = match get_lang() {
                    UiLang::ZhTw => UiLang::En,
                    UiLang::En => UiLang::ZhTw,
                };
                set_lang(next);
                st.config.ui_lang = next;
                let _ = st.config.save();
            }
            HitAction::FocusInput(field) => {
                st.focused_input = Some(field);
                st.caret_visible = true;
                let k = field_key(field);
                let text = input_buf_mut(st, field).clone();
                let dpi = st.dpi;
                let mut meta = st.input_meta.get(&k).cloned().unwrap_or_default();
                let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
                let font = create_ui_font(dpi, 10, FW_NORMAL.0 as i32);
                let rel_x = click_x - (meta.box_rect.left + s(10, dpi));
                let pos = hit_test_char_index(hdc, &text, rel_x, font);
                let _ = DeleteObject(font);
                let _ = windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);

                meta.cursor_char = pos;
                meta.sel_anchor = Some(pos);
                meta.dragging = true;
                st.input_meta.insert(k, meta);
                let _ = SetCapture(hwnd);
                let _ = SetTimer(hwnd, TIMER_CARET_ID, 530, None);
            }
            HitAction::ClearInput(field) => {
                input_buf_mut(st, field).clear();
                let k = field_key(field);
                let m = st.input_meta.entry(k).or_default();
                m.cursor_char = 0;
                m.sel_anchor = None;
                m.dragging = false;
                st.focused_input = Some(field);
                st.caret_visible = true;
            }
            HitAction::SelectRoot(idx) => {
                if idx < st.config.root_folders.len() {
                    st.selected_root_idx = idx;
                }
            }
            HitAction::AddRootFolder => {
                let next = st.config.next_root_folder();
                st.config.root_folders.push(next);
                st.selected_root_idx = st.config.root_folders.len() - 1;
                let _ = st.config.save();
                st.status_msg = tr(
                    "已新增根資料夾，請點擊「套用並更新側邊欄」註冊。",
                    "Root folder added. Click 'Apply & Refresh' to register.",
                )
                .to_string();
                st.status_is_err = false;
            }
            HitAction::DeleteRootFolder(idx) => {
                if st.config.root_folders.len() > 1 && idx < st.config.root_folders.len() {
                    let removed = st.config.root_folders.remove(idx);
                    st.config.subfolders.retain(|s| s.root_id != removed.id);
                    if st.selected_root_idx >= st.config.root_folders.len() {
                        st.selected_root_idx = st.config.root_folders.len() - 1;
                    }
                    let _ = st.config.save();
                    if let Some(dll) = find_shell_dll() {
                        let _ = register_namespace(&st.config, &dll);
                    }
                    st.status_msg = tr("已刪除根資料夾。", "Root folder deleted.").to_string();
                    st.status_is_err = false;
                }
            }
            HitAction::ToggleShowInSidebar => {
                st.config.show_in_sidebar = !st.config.show_in_sidebar;
                let _ = SetTimer(hwnd, TIMER_ANIM_ID, 16, None);
            }
            HitAction::ToggleHideDesktop => {
                st.config.hide_from_desktop = !st.config.hide_from_desktop;
                let _ = SetTimer(hwnd, TIMER_ANIM_ID, 16, None);
            }
            HitAction::TogglePreviewExpand => {
                st.preview_expanded = !st.preview_expanded;
            }
            HitAction::ApplyAll => {
                for r in &mut st.config.root_folders {
                    r.name = r.name.trim().to_string();
                    if r.name.is_empty() {
                        r.name = "自訂資料夾".to_string();
                    }
                    r.target_path = r.target_path.trim().to_string();
                    r.icon = r.icon.trim().to_string();
                }
                if let Err(e) = st.config.save() {
                    st.status_msg = e;
                    st.status_is_err = true;
                } else if let Some(dll) = find_shell_dll() {
                    match register_namespace(&st.config, &dll) {
                        Ok(()) => {
                            st.registered = true;
                            st.status_msg = tr(
                                "已儲存設定並更新檔案總管側邊欄。",
                                "Settings saved and Explorer sidebar refreshed.",
                            )
                            .to_string();
                            st.status_is_err = false;
                        }
                        Err(e) => {
                            st.status_msg = e;
                            st.status_is_err = true;
                        }
                    }
                } else {
                    st.status_msg = tr(
                        "設定已儲存，但找不到 cfd_shell.dll（請先建置專案）。",
                        "Saved, but cfd_shell.dll was not found.",
                    )
                    .to_string();
                    st.status_is_err = true;
                }
            }
            HitAction::Unregister => {
                unregister_namespace();
                st.registered = false;
                st.status_msg = tr("已從檔案總管移除自訂資料夾。", "Unregistered from Explorer.").to_string();
                st.status_is_err = false;
            }
            HitAction::RestartExplorer => match restart_explorer() {
                Ok(()) => {
                    st.status_msg = tr("已重新啟動 Windows 檔案總管。", "Windows Explorer restarted.").to_string();
                    st.status_is_err = false;
                }
                Err(e) => {
                    st.status_msg = e;
                    st.status_is_err = true;
                }
            },
            HitAction::BrowseIconFile => {
                let filter = "Icon / Executable (*.ico;*.dll;*.exe)\0*.ico;*.dll;*.exe\0All Files (*.*)\0*.*\0\0";
                if let Some(p) = prompt_open_file(
                    hwnd,
                    tr("選擇圖示檔案 (.ico / .dll / .exe)", "Select Icon File"),
                    filter,
                ) {
                    let idx = st.selected_root_idx;
                    st.config.root_folders[idx].icon = p;
                }
            }
            HitAction::BrowseRootPath => {
                if let Some(p) = browse_folder_dialog(hwnd) {
                    let idx = st.selected_root_idx;
                    st.config.root_folders[idx].target_path = p;
                }
            }
            HitAction::ImportCustomIcon => {
                let filter = "Images (*.png;*.jpg;*.jpeg;*.bmp;*.webp;*.gif;*.ico)\0*.png;*.jpg;*.jpeg;*.bmp;*.webp;*.gif;*.ico\0All Files (*.*)\0*.*\0\0";
                if let Some(p) = prompt_open_file(
                    hwnd,
                    tr("選擇圖片轉換為 Windows 圖示 (.ico)", "Select Image to Convert to ICO"),
                    filter,
                ) {
                    match import_image_as_ico(Path::new(&p)) {
                        Ok(ico_path) => {
                            let idx = st.selected_root_idx;
                            st.config.root_folders[idx].icon = ico_path.display().to_string();
                            st.custom_icons = list_custom_icons();
                            st.status_msg = tr(
                                "已成功轉換並儲存自訂圖示！",
                                "Custom icon converted and saved!",
                            )
                            .to_string();
                            st.status_is_err = false;
                        }
                        Err(e) => {
                            st.status_msg = e;
                            st.status_is_err = true;
                        }
                    }
                }
            }
            HitAction::SelectIcon(val) => {
                let idx = st.selected_root_idx;
                st.config.root_folders[idx].icon = val;
            }
            HitAction::DeleteCustomIcon(path) => {
                let path_str = path.display().to_string();
                match delete_custom_icon(&path) {
                    Ok(()) => {
                        for r in &mut st.config.root_folders {
                            if r.icon.eq_ignore_ascii_case(&path_str) {
                                r.icon = "shell32.dll,-4".to_string();
                            }
                        }
                        st.icon_previews.remove(&path);
                        st.custom_icons = list_custom_icons();
                        st.status_msg = tr("已刪除自訂圖示。", "Custom icon deleted.").to_string();
                        st.status_is_err = false;
                    }
                    Err(e) => {
                        st.status_msg = e;
                        st.status_is_err = true;
                    }
                }
            }
            HitAction::BrowseSubPath => {
                if let Some(p) = browse_folder_dialog(hwnd) {
                    st.sub_path_buf = p.clone();
                    if st.sub_name_buf.trim().is_empty() {
                        if let Some(fname) = Path::new(&p).file_name().and_then(|n| n.to_str()) {
                            st.sub_name_buf = fname.to_string();
                        }
                    }
                }
            }
            HitAction::SaveSubfolder => {
                let name = st.sub_name_buf.trim().to_string();
                let path = st.sub_path_buf.trim().to_string();
                let root_id = st.config.root_folders[st.selected_root_idx].id.clone();
                if name.is_empty() || path.is_empty() {
                    st.status_msg = tr("請輸入子資料夾名稱與對應路徑。", "Please enter both name and path.").to_string();
                    st.status_is_err = true;
                } else {
                    if let Some(edit_id) = st.editing_id.take() {
                        if let Some(item) = st.config.subfolders.iter_mut().find(|e| e.id == edit_id) {
                            item.name = name;
                            item.target_path = path;
                            item.root_id = root_id;
                        }
                    } else {
                        let id = st.config.next_subfolder_id();
                        st.config.subfolders.push(SubFolderEntry {
                            id,
                            root_id,
                            name,
                            target_path: path,
                            icon_path: String::new(),
                            enabled: true,
                        });
                    }
                    st.sub_name_buf.clear();
                    st.sub_path_buf.clear();
                    let _ = st.config.save();
                    notify_explorer_refresh();
                    st.status_msg = tr("子資料夾清單已更新。", "Subfolder list updated.").to_string();
                    st.status_is_err = false;
                }
            }
            HitAction::CancelEditSubfolder => {
                st.editing_id = None;
                st.sub_name_buf.clear();
                st.sub_path_buf.clear();
            }
            HitAction::ToggleSubEnabled(idx) => {
                if let Some(item) = st.config.subfolders.get_mut(idx) {
                    item.enabled = !item.enabled;
                    let _ = st.config.save();
                    notify_explorer_refresh();
                }
            }
            HitAction::MoveSubUp(idx) => {
                let root_id = st.config.subfolders.get(idx).map(|s| s.root_id.clone());
                if let Some(rid) = root_id {
                    if let Some(prev) = (0..idx).rev().find(|&j| st.config.subfolders[j].root_id == rid) {
                        st.config.subfolders.swap(idx, prev);
                        let _ = st.config.save();
                        notify_explorer_refresh();
                    }
                }
            }
            HitAction::MoveSubDown(idx) => {
                let root_id = st.config.subfolders.get(idx).map(|s| s.root_id.clone());
                if let Some(rid) = root_id {
                    if let Some(next) = ((idx + 1)..st.config.subfolders.len())
                        .find(|&j| st.config.subfolders[j].root_id == rid)
                    {
                        st.config.subfolders.swap(idx, next);
                        let _ = st.config.save();
                        notify_explorer_refresh();
                    }
                }
            }
            HitAction::EditSub(idx) => {
                if let Some(item) = st.config.subfolders.get(idx).cloned() {
                    st.editing_id = Some(item.id);
                    st.sub_name_buf = item.name;
                    st.sub_path_buf = item.target_path;
                }
            }
            HitAction::OpenSub(idx) => {
                if let Some(item) = st.config.subfolders.get(idx) {
                    open_path_in_explorer(&item.target_path);
                }
            }
            HitAction::DeleteSub(idx) => {
                if idx < st.config.subfolders.len() {
                    st.config.subfolders.remove(idx);
                    let _ = st.config.save();
                    notify_explorer_refresh();
                }
            }
            HitAction::OpenConfigDir => {
                let dir = config_dir();
                let _ = std::fs::create_dir_all(&dir);
                if let Some(p) = dir.to_str() {
                    open_path_in_explorer(p);
                }
            }
            HitAction::CleanupAll => match cleanup_all() {
                Ok(()) => {
                    st.registered = false;
                    st.custom_icons.clear();
                    st.icon_previews.clear();
                    st.config = AppConfig::default();
                    st.selected_root_idx = 0;
                    st.status_msg = tr(
                        "已完全清理所有登錄檔、設定檔與自訂圖示！",
                        "All registry entries, configs, and custom icons cleaned!",
                    )
                    .to_string();
                    st.status_is_err = false;
                }
                Err(e) => {
                    st.status_msg = e;
                    st.status_is_err = true;
                }
            },
        }
    });
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let dpi = GetDpiForWindow(hwnd);
            let dpi = if dpi == 0 { 96 } else { dpi };
            let cfg = AppConfig::load();
            set_lang(cfg.ui_lang);
            let anim_sidebar = if cfg.show_in_sidebar { 1.0 } else { 0.0 };
            let anim_hide_desktop = if cfg.hide_from_desktop { 1.0 } else { 0.0 };

            let st = AppState {
                dpi,
                active_tab: TabPage::Sidebar,
                config: cfg,
                selected_root_idx: 0,
                registered: is_registered(),
                preview_expanded: true,
                editing_id: None,
                sub_name_buf: String::new(),
                sub_path_buf: String::new(),
                focused_input: None,
                input_meta: HashMap::new(),
                caret_visible: true,
                custom_icons: list_custom_icons(),
                icon_previews: HashMap::new(),
                status_msg: String::new(),
                status_is_err: false,
                scroll_y: 0,
                max_scroll: 0,
                hover_idx: None,
                hit_regions: Vec::new(),
                anim_sidebar,
                anim_hide_desktop,
            };
            STATE.with(|cell| {
                *cell.borrow_mut() = Some(st);
            });
            LRESULT(0)
        }
        WM_SIZE => {
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = &mut *(lparam.0 as *mut MINMAXINFO);
            mmi.ptMinTrackSize = POINT { x: 860, y: 600 };
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint_window(hwnd);
            LRESULT(0)
        }
        WM_CHAR => {
            let ch_code = wparam.0 as u32;
            let mut handled = false;
            STATE.with(|cell| {
                if let Some(st) = cell.borrow_mut().as_mut() {
                    if let Some((buf, mut meta, k)) = active_input_parts(st) {
                        let char_len = buf.chars().count();
                        meta.cursor_char = meta.cursor_char.min(char_len);
                        if ch_code == 8 {
                            if !delete_selection(buf, &mut meta) && meta.cursor_char > 0 {
                                let b0 = char_to_byte_idx(buf, meta.cursor_char - 1);
                                let b1 = char_to_byte_idx(buf, meta.cursor_char);
                                buf.replace_range(b0..b1, "");
                                meta.cursor_char -= 1;
                            }
                            handled = true;
                        } else if ch_code == 27 {
                            st.focused_input = None;
                            handled = true;
                        } else if ch_code >= 32 {
                            if let Some(c) = char::from_u32(ch_code) {
                                delete_selection(buf, &mut meta);
                                let b = char_to_byte_idx(buf, meta.cursor_char);
                                buf.insert(b, c);
                                meta.cursor_char += 1;
                                meta.sel_anchor = None;
                                handled = true;
                            }
                        }
                        st.input_meta.insert(k, meta);
                        st.caret_visible = true;
                    }
                }
            });
            if handled {
                let _ = InvalidateRect(hwnd, None, false);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_KEYDOWN => {
            let vk = wparam.0 as u32;
            let ctrl = (GetKeyState(0x11) as u16 & 0x8000) != 0;
            let shift = (GetKeyState(0x10) as u16 & 0x8000) != 0;
            let mut handled = false;

            if ctrl && vk == b'V' as u32 {
                if let Some(clip) = read_clipboard_text(hwnd) {
                    let clean: String = clip.chars().filter(|c| *c != '\r' && *c != '\n').collect();
                    STATE.with(|cell| {
                        if let Some(st) = cell.borrow_mut().as_mut() {
                            if let Some((buf, mut meta, k)) = active_input_parts(st) {
                                delete_selection(buf, &mut meta);
                                let b = char_to_byte_idx(buf, meta.cursor_char.min(buf.chars().count()));
                                buf.insert_str(b, &clean);
                                meta.cursor_char += clean.chars().count();
                                meta.sel_anchor = None;
                                st.input_meta.insert(k, meta);
                                st.caret_visible = true;
                                handled = true;
                            }
                        }
                    });
                }
            } else if ctrl && (vk == b'C' as u32 || vk == b'X' as u32) {
                let mut text_to_copy = None;
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        if let Some((buf, mut meta, k)) = active_input_parts(st) {
                            let char_len = buf.chars().count();
                            if let Some((s0, s1)) = get_sel_range(&meta, char_len) {
                                let b0 = char_to_byte_idx(buf, s0);
                                let b1 = char_to_byte_idx(buf, s1);
                                text_to_copy = Some(buf[b0..b1].to_string());
                                if vk == b'X' as u32 {
                                    delete_selection(buf, &mut meta);
                                    st.input_meta.insert(k, meta);
                                }
                            } else {
                                text_to_copy = Some(buf.clone());
                                if vk == b'X' as u32 {
                                    buf.clear();
                                    meta.cursor_char = 0;
                                    meta.sel_anchor = None;
                                    st.input_meta.insert(k, meta);
                                }
                            }
                            handled = true;
                        }
                    }
                });
                if let Some(t) = text_to_copy {
                    copy_to_clipboard(hwnd, &t);
                }
            } else if ctrl && vk == b'A' as u32 {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        if let Some((buf, mut meta, k)) = active_input_parts(st) {
                            meta.sel_anchor = Some(0);
                            meta.cursor_char = buf.chars().count();
                            st.input_meta.insert(k, meta);
                            st.caret_visible = true;
                            handled = true;
                        }
                    }
                });
            } else if matches!(vk, 0x25 | 0x27 | 0x24 | 0x23 | 0x2E) {
                // Left (0x25), Right (0x27), Home (0x24), End (0x23), Delete (0x2E)
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        if let Some((buf, mut meta, k)) = active_input_parts(st) {
                            let char_len = buf.chars().count();
                            meta.cursor_char = meta.cursor_char.min(char_len);
                            if vk == 0x2E {
                                if !delete_selection(buf, &mut meta) && meta.cursor_char < char_len {
                                    let b0 = char_to_byte_idx(buf, meta.cursor_char);
                                    let b1 = char_to_byte_idx(buf, meta.cursor_char + 1);
                                    buf.replace_range(b0..b1, "");
                                }
                            } else {
                                if shift && meta.sel_anchor.is_none() {
                                    meta.sel_anchor = Some(meta.cursor_char);
                                } else if !shift {
                                    meta.sel_anchor = None;
                                }
                                match vk {
                                    0x25 => meta.cursor_char = meta.cursor_char.saturating_sub(1),
                                    0x27 => meta.cursor_char = (meta.cursor_char + 1).min(char_len),
                                    0x24 => meta.cursor_char = 0,
                                    0x23 => meta.cursor_char = char_len,
                                    _ => {}
                                }
                            }
                            st.input_meta.insert(k, meta);
                            st.caret_visible = true;
                            handled = true;
                        }
                    }
                });
            }

            if handled {
                let _ = InvalidateRect(hwnd, None, false);
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut changed = false;
            let mut cur_kind = CursorKind::Arrow;
            let mut dragging_field = None;
            STATE.with(|cell| {
                if let Some(st) = cell.borrow_mut().as_mut() {
                    if let Some(f) = st.focused_input {
                        let k = field_key(f);
                        if st.input_meta.get(&k).map(|m| m.dragging).unwrap_or(false) {
                            dragging_field = Some(f);
                        }
                    }
                    let found = st
                        .hit_regions
                        .iter()
                        .position(|h| rect_contains(&h.rect, x, y));
                    if found != st.hover_idx {
                        st.hover_idx = found;
                        changed = true;
                    }
                    if let Some(idx) = found {
                        cur_kind = st.hit_regions[idx].cursor;
                    }
                }
            });

            if let Some(f) = dragging_field {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        let dpi = st.dpi;
                        let k = field_key(f);
                        let text = input_buf_mut(st, f).clone();
                        if let Some(meta) = st.input_meta.get_mut(&k) {
                            let hdc = windows::Win32::Graphics::Gdi::GetDC(hwnd);
                            let font = create_ui_font(dpi, 10, FW_NORMAL.0 as i32);
                            let rel_x = x - (meta.box_rect.left + s(10, dpi));
                            meta.cursor_char = hit_test_char_index(hdc, &text, rel_x, font);
                            let _ = DeleteObject(font);
                            let _ = windows::Win32::Graphics::Gdi::ReleaseDC(hwnd, hdc);
                        }
                    }
                });
                cur_kind = CursorKind::Ibeam;
                changed = true;
            }

            let idc = match cur_kind {
                CursorKind::Arrow => IDC_ARROW,
                CursorKind::Hand => IDC_HAND,
                CursorKind::Ibeam => IDC_IBEAM,
            };
            if let Ok(cur) = LoadCursorW(None, idc) {
                SetCursor(cur);
            }
            if changed {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let mut was_dragging = false;
            STATE.with(|cell| {
                if let Some(st) = cell.borrow_mut().as_mut() {
                    for m in st.input_meta.values_mut() {
                        if m.dragging {
                            m.dragging = false;
                            if m.sel_anchor == Some(m.cursor_char) {
                                m.sel_anchor = None;
                            }
                            was_dragging = true;
                        }
                    }
                }
            });
            if was_dragging {
                let _ = ReleaseCapture();
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut selected_all = false;
            STATE.with(|cell| {
                if let Some(st) = cell.borrow_mut().as_mut() {
                    let hit = st
                        .hit_regions
                        .iter()
                        .find(|h| rect_contains(&h.rect, x, y))
                        .map(|h| h.action.clone());
                    if let Some(HitAction::FocusInput(field)) = hit {
                        st.focused_input = Some(field);
                        let k = field_key(field);
                        let len = input_buf_mut(st, field).chars().count();
                        let m = st.input_meta.entry(k).or_default();
                        m.sel_anchor = Some(0);
                        m.cursor_char = len;
                        m.dragging = false;
                        st.caret_visible = true;
                        selected_all = true;
                    }
                }
            });
            if selected_all {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lparam.0 & 0xFFFF) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut clicked_action = None;
            STATE.with(|cell| {
                if let Some(st) = cell.borrow().as_ref() {
                    if let Some(h) = st.hit_regions.iter().find(|h| rect_contains(&h.rect, x, y)) {
                        clicked_action = Some(h.action.clone());
                    }
                }
            });
            if let Some(act) = clicked_action {
                handle_action(hwnd, act, x);
            } else {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        st.focused_input = None;
                    }
                });
                let _ = KillTimer(hwnd, TIMER_CARET_ID);
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let mut scrolled = false;
            STATE.with(|cell| {
                if let Some(st) = cell.borrow_mut().as_mut() {
                    if st.max_scroll > 0 {
                        let next = (st.scroll_y - (delta / 3)).clamp(0, st.max_scroll);
                        if next != st.scroll_y {
                            st.scroll_y = next;
                            scrolled = true;
                        }
                    }
                }
            });
            if scrolled {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_ANIM_ID {
                let mut done = true;
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        let step = 0.22f32;
                        let target1 = if st.config.show_in_sidebar { 1.0 } else { 0.0 };
                        let target2 = if st.config.hide_from_desktop { 1.0 } else { 0.0 };
                        let move_towards = |cur: &mut f32, target: f32| -> bool {
                            let diff = target - *cur;
                            if diff.abs() <= step {
                                *cur = target;
                                true
                            } else {
                                *cur += diff.signum() * step;
                                false
                            }
                        };
                        let d1 = move_towards(&mut st.anim_sidebar, target1);
                        let d2 = move_towards(&mut st.anim_hide_desktop, target2);
                        done = d1 && d2;
                    }
                });
                if done {
                    let _ = KillTimer(hwnd, TIMER_ANIM_ID);
                }
                let _ = InvalidateRect(hwnd, None, false);
            } else if wparam.0 == TIMER_CARET_ID {
                STATE.with(|cell| {
                    if let Some(st) = cell.borrow_mut().as_mut() {
                        st.caret_visible = !st.caret_visible;
                    }
                });
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            refresh_system_theme(Some(hwnd));
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        refresh_system_theme(None);

        let hinst = GetModuleHandleW(None).unwrap_or_default();
        let class_name = w!("CustomerFDManagerWindow");
        let hicon = create_app_hicon(accent_rgb()).unwrap_or_default();

        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinst.into(),
            hIcon: hicon,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        let w = 1020;
        let h = 720;
        let x = (sw - w) / 2;
        let y = (sh - h) / 2;
        let title_w = to_wide("CustomerFD 控制中心");

        let hwnd = CreateWindowExW(
            Default::default(),
            class_name,
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPED
                | WS_CAPTION
                | WS_SYSMENU
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_THICKFRAME
                | WS_CLIPCHILDREN,
            x,
            y,
            w,
            h,
            None,
            None,
            hinst,
            None,
        )
        .expect("CreateWindowExW failed");

        refresh_system_theme(Some(hwnd));
        let mica_alt: i32 = 4;
        if DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &mica_alt as *const _ as _, 4).is_err() {
            let mica: i32 = 2;
            let _ = DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &mica as *const _ as _, 4);
        }
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        let _ = ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CoUninitialize();
    }
}
