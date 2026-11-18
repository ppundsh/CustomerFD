//! Win32 helpers: common dialogs, clipboard, opening paths and URLs, shell icon extraction.

use crate::i18n::tr;
use cfd_ui::wstr::to_wide;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HINSTANCE, HWND};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_INPROC_SERVER};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, SHDefExtractIconW, ShellExecuteW, FOS_FORCEFILESYSTEM,
    FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{HICON, SW_SHOW};

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
    fn OpenClipboard(hwnd: HWND) -> windows::Win32::Foundation::BOOL;
    fn CloseClipboard() -> windows::Win32::Foundation::BOOL;
    fn EmptyClipboard() -> windows::Win32::Foundation::BOOL;
    fn SetClipboardData(format: u32, mem: HANDLE) -> HANDLE;
    fn GetClipboardData(format: u32) -> HANDLE;
}

#[link(name = "kernel32")]
extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> HGLOBAL;
    fn GlobalLock(mem: HGLOBAL) -> *mut std::ffi::c_void;
    fn GlobalUnlock(mem: HGLOBAL) -> windows::Win32::Foundation::BOOL;
}

const CF_UNICODETEXT: u32 = 13;

/// Clipboard text (first line only, at most 4096 UTF-16 units).
pub unsafe fn read_clipboard_text(hwnd: HWND) -> Option<String> {
    if !OpenClipboard(hwnd).as_bool() {
        return None;
    }
    let mut out = None;
    let handle = GetClipboardData(CF_UNICODETEXT);
    if !handle.is_invalid() {
        let ptr = GlobalLock(HGLOBAL(handle.0));
        if !ptr.is_null() {
            let wptr = ptr.cast::<u16>();
            let mut len = 0usize;
            while len < 4096 && *wptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(wptr, len));
            out = Some(s.lines().next().unwrap_or("").to_string());
            let _ = GlobalUnlock(HGLOBAL(handle.0));
        }
    }
    let _ = CloseClipboard();
    out
}

pub unsafe fn copy_to_clipboard(hwnd: HWND, text: &str) {
    if !OpenClipboard(hwnd).as_bool() {
        return;
    }
    let _ = EmptyClipboard();
    let wide = to_wide(text);
    let bytes = wide.len() * 2;
    let mem = GlobalAlloc(0x0042 /* GMEM_MOVEABLE | GMEM_ZEROINIT */, bytes);
    if !mem.is_invalid() {
        let ptr = GlobalLock(mem);
        if !ptr.is_null() {
            std::ptr::copy_nonoverlapping(wide.as_ptr().cast::<u8>(), ptr.cast::<u8>(), bytes);
            let _ = GlobalUnlock(mem);
            let _ = SetClipboardData(CF_UNICODETEXT, HANDLE(mem.0));
        }
    }
    let _ = CloseClipboard();
}

/// Classic open-file dialog. `filter` uses the `"Name\0*.ext\0...\0\0"` format.
pub unsafe fn prompt_open_file(hwnd: HWND, title: &str, filter: &str) -> Option<String> {
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
        // OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR
        Flags: 0x0000_1000 | 0x0000_0800 | 0x0000_0008,
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

/// Modern folder picker.
pub unsafe fn browse_folder_dialog(owner: HWND, title: &str) -> Option<String> {
    let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let opts = dialog.GetOptions().ok()?;
    dialog.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM | FOS_PATHMUSTEXIST).ok()?;
    let title_w = to_wide(title);
    let _ = dialog.SetTitle(PCWSTR(title_w.as_ptr()));
    dialog.Show(owner).ok()?;
    let item = dialog.GetResult().ok()?;
    let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let path = pwstr.to_string().ok();
    CoTaskMemFree(Some(pwstr.0 as *const _));
    path
}

/// Opens a folder, file or URL with the shell.
pub unsafe fn shell_open(target: &str) {
    let path_w = to_wide(target);
    let op_w = to_wide("open");
    ShellExecuteW(HWND::default(), PCWSTR(op_w.as_ptr()), PCWSTR(path_w.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOW);
}

/// Splits an icon location `"C:\path\file.dll,-4"` into path and index (index 0 when absent).
pub fn split_icon_location(loc: &str) -> (String, i32) {
    let t = loc.trim().trim_matches('"');
    if let Some((p, idx)) = t.rsplit_once(',') {
        if let Ok(i) = idx.trim().parse::<i32>() {
            return (p.trim().trim_matches('"').to_string(), i);
        }
    }
    (t.to_string(), 0)
}

fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            match std::env::var(name) {
                Ok(v) => out.push_str(&v),
                Err(_) => {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
            }
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            rest = "";
        }
    }
    out.push_str(rest);
    out
}

/// Loads the icon at `loc` (".ico", ".dll,-id", ".exe,index") at `size` px. Caller destroys it.
pub unsafe fn load_shell_icon(loc: &str, size: i32) -> Option<HICON> {
    let (path, index) = split_icon_location(loc);
    let mut path = expand_env(&path);
    if path.is_empty() {
        return None;
    }
    if !path.contains('\\') && !path.contains('/') {
        let sysroot = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let sys32 = Path::new(&sysroot).join("System32").join(&path);
        if sys32.exists() {
            path = sys32.display().to_string();
        }
    }
    let path_w = to_wide(&path);
    let mut large = HICON::default();
    let sz = size.clamp(16, 256) as u32;
    let hr = SHDefExtractIconW(PCWSTR(path_w.as_ptr()), index, 0, Some(&mut large), None, sz);
    if hr.is_ok() && !large.is_invalid() {
        Some(large)
    } else {
        None
    }
}

/// Folder-picker title.
pub fn folder_dialog_title() -> &'static str {
    tr("選擇目標資料夾", "Select Target Folder")
}
