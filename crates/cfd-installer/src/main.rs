// CustomerFD — Windows 11 Explorer sidebar custom folders.
// Copyright (C) 2026 ppundsh
// SPDX-License-Identifier: GPL-3.0-or-later

//! `CustomerFD-Setup.exe`: per-user installer, in-place updater and uninstaller.
//!
//! ```text
//! (no args)                                   graphical installer
//! --quiet | --silent | /s  [--dir <folder>]   silent install
//! --uninstall [--quiet] [--purge]             uninstall (--purge also deletes settings)
//! --update --dir <folder> [--wait-pid <pid>]  in-place update (used by the control center)
//! --check                                     environment check (+ self-test when installed)
//! --lang zh-TW | en                           UI language
//! ```
//!
//! Exit codes of the quiet modes: 0 ok, 1 install failed, 2 environment not supported,
//! 3 installed but the self-test failed.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod env_check;
mod install;

use cfd_core::config::{AppConfig, UiLang};
use cfd_ui::gdi::{draw_text, rect, Fonts};
use cfd_ui::scale;
use cfd_ui::theme::{refresh_system_theme, theme};
use cfd_ui::widgets::{self, ButtonKind};
use cfd_ui::wstr::to_wide;
use install::{Options, APP_NAME, VERSION};
use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, ShellExecuteW, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
    SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::*;

static LANG_EN: AtomicU8 = AtomicU8::new(0);

pub fn tr<'a>(zh: &'a str, en: &'a str) -> &'a str {
    if LANG_EN.load(Ordering::Relaxed) == 1 {
        en
    } else {
        zh
    }
}

fn set_lang_en(en: bool) {
    LANG_EN.store(en as u8, Ordering::Relaxed);
}

fn msg_box(hwnd: HWND, text: &str, flags: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
    let t = to_wide(text);
    let c = to_wide(tr("CustomerFD 安裝程式", "CustomerFD Setup"));
    // SAFETY: NUL-terminated strings that outlive the call.
    unsafe { MessageBoxW(hwnd, PCWSTR(t.as_ptr()), PCWSTR(c.as_ptr()), flags) }
}

struct Args {
    quiet: bool,
    uninstall: bool,
    purge: bool,
    update: bool,
    dir: Option<PathBuf>,
    wait_pid: Option<u32>,
    lang: Option<bool>,
    check: bool,
}

fn parse_args() -> Args {
    let mut a = Args {
        quiet: false,
        uninstall: false,
        purge: false,
        update: false,
        dir: None,
        wait_pid: None,
        lang: None,
        check: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(s) = it.next() {
        match s.to_ascii_lowercase().as_str() {
            "--quiet" | "--silent" | "/s" | "/quiet" => a.quiet = true,
            "--uninstall" | "/uninstall" => a.uninstall = true,
            "--purge" => a.purge = true,
            "--update" => a.update = true,
            "--check" | "/check" => a.check = true,
            "--dir" => a.dir = it.next().map(PathBuf::from),
            "--wait-pid" => a.wait_pid = it.next().and_then(|p| p.parse().ok()),
            "--lang" => a.lang = it.next().map(|l| l.to_ascii_lowercase().starts_with("en")),
            _ => {}
        }
    }
    a
}

// ---------------------------------------------------------------------------------------------
// GUI

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Act {
    ChangeDir,
    Shortcut,
    Launch,
    Register,
    Install,
    Cancel,
    Lang,
    License,
}

struct Ui {
    dpi: u32,
    dir: PathBuf,
    shortcut: bool,
    launch: bool,
    register: bool,
    existing: Option<String>,
    regions: Vec<(RECT, Act)>,
    mouse: (i32, i32),
    pressed: Option<Act>,
    busy: bool,
    /// Environment check results, shown as info bars above the licence line.
    findings: Vec<env_check::Finding>,
}

thread_local! {
    static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
}

fn with_ui<R>(f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    UI.with(|c| c.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

impl Ui {
    fn hot(&mut self, rc: RECT, a: Act) -> (bool, bool) {
        self.regions.push((rc, a));
        let h = cfd_ui::gdi::contains(&rc, self.mouse.0, self.mouse.1) && !self.busy;
        (h, h && self.pressed == Some(a))
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right, rc.bottom);
    let mem = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, w, h);
    let old = SelectObject(mem, bmp);
    with_ui(|ui| draw(ui, mem, w, h));
    let _ = BitBlt(hdc, 0, 0, w, h, mem, 0, 0, SRCCOPY);
    SelectObject(mem, old);
    let _ = DeleteObject(bmp);
    let _ = DeleteDC(mem);
    let _ = EndPaint(hwnd, &ps);
}

const LEFT_VC: DRAW_TEXT_FORMAT =
    DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0);
const PATH_VC: DRAW_TEXT_FORMAT =
    DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_PATH_ELLIPSIS.0);

unsafe fn draw(ui: &mut Ui, hdc: HDC, w: i32, h: i32) {
    let dpi = ui.dpi;
    let th = theme();
    let fonts = Fonts::new(hdc, dpi);
    ui.regions.clear();
    cfd_ui::gdi::fill_rect(hdc, &rect(0, 0, w, h), th.win_bg);
    let m = scale(24, dpi);
    let cw = w - m * 2;

    // header
    let isz = scale(48, dpi);
    let me = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let mw = to_wide(&me);
    let mut icon = HICON::default();
    let _ = windows::Win32::UI::Shell::SHDefExtractIconW(
        PCWSTR(mw.as_ptr()),
        0,
        0,
        Some(&mut icon),
        None,
        isz as u32,
    );
    if !icon.is_invalid() {
        let _ = DrawIconEx(hdc, m, m, icon, isz, isz, 0, None, DI_NORMAL);
        let _ = DestroyIcon(icon);
    }
    let tx = m + isz + scale(16, dpi);
    draw_text(
        hdc,
        APP_NAME,
        &rect(tx, m - scale(2, dpi), cw, scale(30, dpi)),
        th.text_bright,
        fonts.hero,
        LEFT_VC,
    );
    let sub = match &ui.existing {
        Some(v) => format!(
            "{} {VERSION}  ·  {} {v}",
            tr("版本", "Version"),
            tr("已安裝", "installed:")
        ),
        None => format!("{} {VERSION}", tr("版本", "Version")),
    };
    draw_text(
        hdc,
        &sub,
        &rect(tx, m + scale(28, dpi), cw, scale(20, dpi)),
        th.text_secondary,
        fonts.small,
        LEFT_VC,
    );

    // location card
    let mut y = m + isz + scale(24, dpi);
    let card_h = scale(68 + 16, dpi);
    let card = rect(m, y, cw, card_h);
    widgets::card(hdc, dpi, &card);
    let pad = scale(16, dpi);
    draw_text(
        hdc,
        tr("安裝位置", "Install location"),
        &rect(m + pad, y + scale(12, dpi), cw - pad * 2, scale(22, dpi)),
        th.text_bright,
        fonts.body,
        LEFT_VC,
    );
    let bl = tr("變更...", "Change...");
    let bw = widgets::button_width(hdc, &fonts, dpi, bl, false);
    let brc = rect(
        m + cw - pad - bw,
        y + (card_h - scale(32, dpi)) / 2,
        bw,
        scale(32, dpi),
    );
    draw_text(
        hdc,
        &ui.dir.display().to_string(),
        &rect(
            m + pad,
            y + scale(38, dpi),
            cw - pad * 3 - bw,
            scale(28, dpi),
        ),
        th.text_muted,
        fonts.small,
        PATH_VC,
    );
    let (hv, pr) = ui.hot(brc, Act::ChangeDir);
    widgets::button(
        hdc,
        &fonts,
        dpi,
        &brc,
        bl,
        ButtonKind::Standard,
        hv,
        pr,
        ui.busy || ui.existing.is_some(),
    );
    y += card_h + scale(12, dpi);

    // options card
    let opts: [(Act, &str, bool); 3] = [
        (
            Act::Register,
            tr(
                "立即加入檔案總管側邊欄",
                "Add to the File Explorer sidebar now",
            ),
            ui.register,
        ),
        (
            Act::Shortcut,
            tr("建立「開始」功能表捷徑", "Create a Start menu shortcut"),
            ui.shortcut,
        ),
        (
            Act::Launch,
            tr(
                "完成後開啟 CustomerFD 控制中心",
                "Open the CustomerFD control center when done",
            ),
            ui.launch,
        ),
    ];
    let row = scale(40, dpi);
    let card = rect(m, y, cw, row * opts.len() as i32 + scale(12, dpi));
    widgets::card(hdc, dpi, &card);
    let mut ry = y + scale(6, dpi);
    for (a, label, checked) in opts {
        let r = rect(m + pad, ry, cw - pad * 2, row);
        let (hv, _) = ui.hot(r, a);
        let cb = scale(18, dpi);
        widgets::checkbox(
            hdc,
            r.left,
            ry + (row - cb) / 2,
            cb,
            checked,
            hv,
            ui.busy,
            1.0,
        );
        draw_text(
            hdc,
            label,
            &rect(
                r.left + cb + scale(12, dpi),
                ry,
                r.right - r.left - cb - scale(12, dpi),
                row,
            ),
            th.text_primary,
            fonts.body,
            LEFT_VC,
        );
        ry += row;
    }
    y = card.bottom + scale(12, dpi);

    // environment check
    for f in &ui.findings {
        let sev = match f.level {
            env_check::Level::Blocker => widgets::Severity::Error,
            env_check::Level::Warning => widgets::Severity::Warning,
        };
        let bh = widgets::info_bar_height(hdc, &fonts, dpi, cw, &f.text);
        let _ = widgets::info_bar(
            hdc,
            &fonts,
            dpi,
            &rect(m, y, cw, bh),
            sev,
            &f.text,
            false,
            false,
        );
        y += bh + scale(8, dpi);
    }

    // licence line
    let lic = tr(
        "CustomerFD 以 GNU GPL v3.0 或更新版本授權釋出。Copyright © 2026 ppundsh",
        "CustomerFD is licensed under the GNU GPL v3.0 or later. Copyright © 2026 ppundsh",
    );
    draw_text(
        hdc,
        lic,
        &rect(m, y, cw, scale(22, dpi)),
        th.text_muted,
        fonts.small,
        LEFT_VC,
    );

    // footer
    let fh = scale(72, dpi);
    let foot = rect(0, h - fh, w, fh);
    cfd_ui::gdi::fill_rect(hdc, &foot, th.sub_card_bg);
    cfd_ui::gdi::hline(hdc, 0, w, foot.top, th.card_border);
    let bh = scale(32, dpi);
    let by = foot.top + (fh - bh) / 2;
    let ll = if LANG_EN.load(Ordering::Relaxed) == 1 {
        "繁體中文"
    } else {
        "English"
    };
    let lw = widgets::button_width(hdc, &fonts, dpi, ll, true);
    let lrc = rect(m, by, lw, bh);
    let (hv, pr) = ui.hot(lrc, Act::Lang);
    widgets::subtle_button(
        hdc,
        &fonts,
        dpi,
        &lrc,
        Some(widgets::GLYPH_GLOBE),
        ll,
        hv,
        pr,
        ui.busy,
    );
    let lic_l = tr("授權條款", "License");
    let lcw = widgets::button_width(hdc, &fonts, dpi, lic_l, false);
    let lcrc = rect(lrc.right + scale(4, dpi), by, lcw, bh);
    let (hv, pr) = ui.hot(lcrc, Act::License);
    widgets::subtle_button(hdc, &fonts, dpi, &lcrc, None, lic_l, hv, pr, ui.busy);
    let bw = scale(120, dpi);
    let crc = rect(w - m - bw, by, bw, bh);
    let irc = rect(crc.left - scale(8, dpi) - bw, by, bw, bh);
    let il = if ui.busy {
        tr("正在安裝...", "Installing...")
    } else if ui.existing.is_some() {
        tr("更新", "Update")
    } else {
        tr("安裝", "Install")
    };
    let blocked = env_check::has_blocker(&ui.findings);
    let (hv, pr) = ui.hot(irc, Act::Install);
    widgets::button(
        hdc,
        &fonts,
        dpi,
        &irc,
        il,
        ButtonKind::Accent,
        hv,
        pr,
        ui.busy || blocked,
    );
    let (hv, pr) = ui.hot(crc, Act::Cancel);
    widgets::button(
        hdc,
        &fonts,
        dpi,
        &crc,
        tr("取消", "Cancel"),
        ButtonKind::Standard,
        hv,
        pr,
        ui.busy,
    );
}

unsafe fn pick_folder(owner: HWND, start: &std::path::Path) -> Option<PathBuf> {
    let dialog: IFileOpenDialog =
        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
    let opts = dialog.GetOptions().ok()?;
    dialog
        .SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
        .ok()?;
    let _ = start;
    dialog.Show(owner).ok()?;
    let item = dialog.GetResult().ok()?;
    let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
    let s = p.to_string().ok();
    CoTaskMemFree(Some(p.0 as *const _));
    s.map(|s| PathBuf::from(s).join(APP_NAME))
}

unsafe fn act(hwnd: HWND, a: Act) {
    match a {
        Act::ChangeDir => {
            let cur = with_ui(|u| u.dir.clone()).unwrap_or_default();
            if let Some(d) = pick_folder(hwnd, &cur) {
                with_ui(|u| u.dir = d);
            }
        }
        Act::Shortcut => {
            with_ui(|u| u.shortcut = !u.shortcut);
        }
        Act::Launch => {
            with_ui(|u| u.launch = !u.launch);
        }
        Act::Register => {
            with_ui(|u| u.register = !u.register);
        }
        Act::Lang => set_lang_en(LANG_EN.load(Ordering::Relaxed) == 0),
        Act::License => {
            let url = to_wide("https://github.com/ppundsh/CustomerFD/blob/main/LICENSE");
            let op = to_wide("open");
            ShellExecuteW(
                HWND::default(),
                PCWSTR(op.as_ptr()),
                PCWSTR(url.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOW,
            );
        }
        Act::Cancel => {
            let _ = DestroyWindow(hwnd);
            return;
        }
        Act::Install => {
            if with_ui(|u| env_check::has_blocker(&u.findings)).unwrap_or(false) {
                return;
            }
            let Some((dir, opts, launch, update)) = with_ui(|u| {
                u.busy = true;
                (
                    u.dir.clone(),
                    Options {
                        shortcut: u.shortcut,
                        register: u.register,
                    },
                    u.launch,
                    u.existing.is_some(),
                )
            }) else {
                return;
            };
            let _ = InvalidateRect(hwnd, None, false);
            let _ = UpdateWindow(hwnd);
            if update {
                install::close_running_manager();
            }
            match install::install(&dir, opts, false) {
                Ok(()) => {
                    if let Err(e) = env_check::verify_install(&dir, opts.register) {
                        msg_box(
                            hwnd,
                            &format!("{}\n\n{e}", self_test_failed()),
                            MB_OK | MB_ICONWARNING,
                        );
                    }
                    if launch {
                        install::launch_manager(&dir, false);
                    } else {
                        msg_box(
                            hwnd,
                            tr("CustomerFD 已安裝完成。", "CustomerFD has been installed."),
                            MB_OK | MB_ICONINFORMATION,
                        );
                    }
                    let _ = DestroyWindow(hwnd);
                    return;
                }
                Err(e) => {
                    msg_box(
                        hwnd,
                        &format!("{}\n\n{e}", tr("安裝失敗。", "Installation failed.")),
                        MB_OK | MB_ICONERROR,
                    );
                    with_ui(|u| u.busy = false);
                }
            }
        }
    }
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let xy = || {
        (
            (lp.0 & 0xFFFF) as i16 as i32,
            ((lp.0 >> 16) & 0xFFFF) as i16 as i32,
        )
    };
    match msg {
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            with_ui(|u| u.mouse = xy());
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let (x, y) = xy();
            with_ui(|u| {
                u.mouse = (x, y);
                u.pressed = if u.busy {
                    None
                } else {
                    u.regions
                        .iter()
                        .rev()
                        .find(|(r, _)| cfd_ui::gdi::contains(r, x, y))
                        .map(|(_, a)| *a)
                };
            });
            let _ = SetCapture(hwnd);
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let (x, y) = xy();
            let _ = ReleaseCapture();
            let fire = with_ui(|u| {
                let p = u.pressed.take();
                let under = u
                    .regions
                    .iter()
                    .rev()
                    .find(|(r, _)| cfd_ui::gdi::contains(r, x, y))
                    .map(|(_, a)| *a);
                (p.is_some() && p == under).then_some(p).flatten()
            })
            .flatten();
            if let Some(a) = fire {
                act(hwnd, a);
            }
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            refresh_system_theme(Some(hwnd));
            let _ = InvalidateRect(hwnd, None, false);
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        WM_DPICHANGED => {
            with_ui(|u| u.dpi = ((wp.0 & 0xFFFF) as u32).max(96));
            let r = &*(lp.0 as *const RECT);
            let _ = SetWindowPos(
                hwnd,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn run_gui() {
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let class = w!("CustomerFDSetupWindow");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: hinst.into(),
        hIcon: LoadIconW(hinst, PCWSTR(MAKEINTRESOURCE_1)).unwrap_or_default(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: class,
        ..Default::default()
    };
    RegisterClassW(&wc);
    let dpi = GetDpiForSystem().max(96);
    let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    let findings = env_check::check();
    let extra = {
        let hdc = GetDC(None);
        let fonts = Fonts::new(hdc, dpi);
        let w = scale(600 - 48, dpi);
        let h: i32 = findings
            .iter()
            .map(|f| widgets::info_bar_height(hdc, &fonts, dpi, w, &f.text) + scale(8, dpi))
            .sum();
        ReleaseDC(None, hdc);
        h
    };
    let mut r = rect(0, 0, scale(600, dpi), scale(440, dpi) + extra);
    let _ = AdjustWindowRectEx(&mut r, style, false, WINDOW_EX_STYLE::default());
    let (ww, wh) = (r.right - r.left, r.bottom - r.top);
    let x = (GetSystemMetrics(SM_CXSCREEN) - ww) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - wh) / 2;
    let existing = install::installed_version();
    let dir = install::installed_dir().unwrap_or_else(install::default_install_dir);
    UI.with(|c| {
        *c.borrow_mut() = Some(Ui {
            dpi,
            dir,
            shortcut: true,
            launch: true,
            register: true,
            existing,
            regions: Vec::new(),
            mouse: (-1, -1),
            pressed: None,
            busy: false,
            findings,
        })
    });
    let title = to_wide(tr("CustomerFD 安裝程式", "CustomerFD Setup"));
    let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class,
        PCWSTR(title.as_ptr()),
        style,
        x,
        y,
        ww,
        wh,
        None,
        None,
        hinst,
        None,
    ) else {
        return;
    };
    with_ui(|u| u.dpi = GetDpiForWindow(hwnd).max(96));
    refresh_system_theme(Some(hwnd));
    let _ = ShowWindow(hwnd, SW_SHOW);
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).into() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

/// `MAKEINTRESOURCEW(1)`: the application icon resource id.
#[allow(clippy::manual_dangling_ptr)]
const MAKEINTRESOURCE_1: *const u16 = 1 as *const u16;

fn main() {
    let args = parse_args();
    let en = args
        .lang
        .unwrap_or_else(|| AppConfig::load().ui_lang == UiLang::En);
    set_lang_en(en);
    // SAFETY: standard process start-up on the UI thread.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    refresh_system_theme(None);

    if args.check {
        let findings = env_check::check();
        let mut text = if findings.is_empty() {
            tr(
                "這部電腦符合 CustomerFD 的執行環境。",
                "This PC meets CustomerFD's requirements.",
            )
            .to_string()
        } else {
            env_check::report(&findings)
        };
        let mut code = if env_check::has_blocker(&findings) {
            2
        } else {
            0
        };
        if let Some(dir) = install::installed_dir() {
            let test = env_check::verify_install(&dir, cfd_core::registry::is_registered());
            text.push_str("\n\n");
            match test {
                Ok(()) => text.push_str(tr(
                    "已安裝的版本通過自我測試。",
                    "The installed version passed the self-test.",
                )),
                Err(e) => {
                    text.push_str(&format!("{}\n{e}", self_test_failed()));
                    code = code.max(3);
                }
            }
        }
        if !args.quiet {
            let icon = if code == 0 {
                MB_ICONINFORMATION
            } else {
                MB_ICONWARNING
            };
            msg_box(HWND::default(), &text, MB_OK | icon);
        }
        std::process::exit(code);
    }

    if args.uninstall {
        if !args.quiet {
            let q = tr(
                "要從這部電腦移除 CustomerFD 嗎？",
                "Remove CustomerFD from this PC?",
            );
            if msg_box(HWND::default(), q, MB_YESNO | MB_ICONQUESTION) != IDYES {
                return;
            }
        }
        let purge = args.purge
            || (!args.quiet
                && msg_box(
                    HWND::default(),
                    tr("也要刪除設定與自訂圖示嗎？\n（選「否」會保留，重新安裝後可繼續使用）", "Also delete your settings and custom icons?\n(Choose No to keep them for a later reinstall.)"),
                    MB_YESNO | MB_ICONQUESTION,
                ) == IDYES);
        match install::uninstall(purge) {
            Ok(()) if !args.quiet => {
                msg_box(HWND::default(), tr("CustomerFD 已移除。重新啟動檔案總管後側邊欄項目會完全消失。", "CustomerFD was removed. Restart File Explorer to clear the sidebar completely."), MB_OK | MB_ICONINFORMATION);
            }
            Err(e) if !args.quiet => {
                msg_box(HWND::default(), &e, MB_OK | MB_ICONERROR);
            }
            _ => {}
        }
        return;
    }

    if args.update {
        if let Some(pid) = args.wait_pid {
            install::wait_for_pid(pid);
        }
        let dir = args
            .dir
            .clone()
            .or_else(install::installed_dir)
            .unwrap_or_else(install::default_install_dir);
        match install::install(
            &dir,
            Options {
                shortcut: false,
                register: true,
            },
            true,
        ) {
            Ok(()) => {
                if let Err(e) = env_check::verify_install(&dir, cfd_core::registry::is_registered())
                {
                    msg_box(
                        HWND::default(),
                        &format!("{}\n\n{e}", self_test_failed()),
                        MB_OK | MB_ICONWARNING,
                    );
                }
                install::launch_manager(&dir, true)
            }
            Err(e) => {
                msg_box(
                    HWND::default(),
                    &format!(
                        "{}\n\n{e}",
                        tr(
                            "更新失敗，將開啟原本的版本。",
                            "The update failed; the previous version will be opened."
                        )
                    ),
                    MB_OK | MB_ICONERROR,
                );
                install::launch_manager(&dir, false);
            }
        }
        return;
    }

    if args.quiet {
        if env_check::has_blocker(&env_check::check()) {
            std::process::exit(2);
        }
        let dir = args
            .dir
            .clone()
            .or_else(install::installed_dir)
            .unwrap_or_else(install::default_install_dir);
        let update = install::installed_dir().is_some();
        if update {
            install::close_running_manager();
        }
        if install::install(
            &dir,
            Options {
                shortcut: true,
                register: true,
            },
            false,
        )
        .is_err()
        {
            std::process::exit(1);
        }
        if env_check::verify_install(&dir, true).is_err() {
            std::process::exit(3);
        }
        return;
    }

    // SAFETY: GUI on the main thread.
    unsafe { run_gui() };
}

fn self_test_failed() -> &'static str {
    tr(
        "CustomerFD 已安裝，但自我測試發現問題，側邊欄可能無法正常顯示：",
        "CustomerFD is installed, but the self-test found a problem; the sidebar may not work:",
    )
}
