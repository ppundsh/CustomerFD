// CustomerFD — Windows 11 Explorer sidebar custom folders.
// Copyright (C) 2026 ppundsh
// SPDX-License-Identifier: GPL-3.0-or-later

//! Environment check before installing and a self-test after installing.
//!
//! Before: is this a Windows that can run the extension (build, CPU architecture, an Explorer
//! shell, not an elevated session that would write another account's settings)?
//! After: is the sidebar registered to the files just installed, and can the extension be
//! loaded and instantiated in this environment?

use crate::install::SHELL_DLL;
use crate::tr;
use cfd_core::config::AppConfig;
use cfd_core::registry::read_reg_sz;
use std::path::Path;
use windows::core::{w, GUID};
use windows::Win32::System::Com::{CoCreateInstance, CoFreeUnusedLibraries, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use windows::Win32::UI::Shell::{IShellFolder, IsUserAnAdmin};
use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

/// Oldest supported Windows build (Windows 10 1803: Acrylic backdrop, dark mode APIs).
pub const MIN_BUILD: u32 = 17134;
/// First Windows 11 build (Mica backdrop, sidebar layout the UI is designed for).
pub const WIN11_BUILD: u32 = 22000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    /// Installing would not work: the Install button is disabled, quiet installs exit with 2.
    Blocker,
    /// Works, with a caveat the user should know about.
    Warning,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub level: Level,
    pub text: String,
}

/// Facts about the machine, gathered once (kept separate so the rules can be unit-tested).
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub build: Option<u32>,
    /// Native processor architecture as Windows reports it (AMD64, ARM64, x86).
    pub native_arch: Option<String>,
    pub elevated: bool,
    pub explorer_shell: bool,
}

pub fn gather() -> Facts {
    let nt = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let build = read_reg_sz(HKEY_LOCAL_MACHINE, nt, Some("CurrentBuildNumber"))
        .and_then(|b| b.trim().parse().ok());
    // The machine-wide value is the native architecture even when read from an emulated
    // x64 process on ARM64 Windows (the per-process environment variable is not).
    let native_arch = read_reg_sz(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        Some("PROCESSOR_ARCHITECTURE"),
    )
    .map(|s| s.trim().to_ascii_uppercase());
    // SAFETY: plain Win32 queries without out-parameters to manage.
    let (elevated, explorer_shell) = unsafe {
        (
            IsUserAnAdmin().as_bool(),
            FindWindowW(w!("Shell_TrayWnd"), None).is_ok_and(|h| !h.is_invalid()),
        )
    };
    Facts {
        build,
        native_arch,
        elevated,
        explorer_shell,
    }
}

/// Rules applied to the gathered facts.
pub fn evaluate(f: &Facts) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut push = |level, text: String| out.push(Finding { level, text });
    match f.build {
        Some(b) if b < MIN_BUILD => push(
            Level::Blocker,
            format!(
                "{} {b}{}",
                tr("這個 Windows 版本太舊（組建", "This Windows version is too old (build"),
                tr(
                    "）。CustomerFD 需要 Windows 11，或 Windows 10 1803（組建 17134）以上。",
                    "). CustomerFD needs Windows 11, or Windows 10 1803 (build 17134) or later."
                )
            ),
        ),
        Some(b) if b < WIN11_BUILD => push(
            Level::Warning,
            tr(
                "這是 Windows 10：功能可用，但介面背景改用 Acrylic，檔案總管側邊欄的外觀也和 Windows 11 不同。",
                "This is Windows 10: everything works, but the window backdrop uses Acrylic and the File Explorer sidebar looks different from Windows 11.",
            )
            .to_string(),
        ),
        Some(_) => {}
        None => push(
            Level::Warning,
            tr(
                "無法判斷 Windows 版本，將繼續安裝。",
                "Could not determine the Windows version; installing anyway.",
            )
            .to_string(),
        ),
    }
    match f.native_arch.as_deref() {
        Some("AMD64") | None => {}
        Some("ARM64") => push(
            Level::Blocker,
            tr(
                "這是 ARM64 版 Windows。CustomerFD 的檔案總管擴充套件是 x64 版本，ARM64 的檔案總管無法載入，側邊欄不會出現。",
                "This is Windows on ARM64. CustomerFD's File Explorer extension is x64, which the ARM64 File Explorer cannot load, so the sidebar would stay empty.",
            )
            .to_string(),
        ),
        Some(other) => push(
            Level::Blocker,
            format!(
                "{} ({other})",
                tr(
                    "CustomerFD 只支援 64 位元（x64）Windows。",
                    "CustomerFD supports 64-bit (x64) Windows only."
                )
            ),
        ),
    }
    if !f.explorer_shell {
        push(
            Level::Warning,
            tr(
                "沒有偵測到檔案總管的工作列（例如遠端應用程式、Server Core 或替代殼層）。安裝可以完成，但要在一般的 Windows 桌面才看得到側邊欄。",
                "No File Explorer taskbar was found (for example RemoteApp, Server Core or a replacement shell). Setup can finish, but the sidebar only shows on a regular Windows desktop.",
            )
            .to_string(),
        );
    }
    if f.elevated {
        push(
            Level::Warning,
            tr(
                "安裝程式正以系統管理員身分執行。CustomerFD 只安裝給目前的帳號；若這是另一個管理員帳號，側邊欄會裝到那個帳號，而不是你登入的帳號。建議以一般方式執行安裝程式。",
                "Setup is running as administrator. CustomerFD installs for the current account only; if that is a different administrator account, the sidebar goes to that account instead of yours. Run Setup normally instead.",
            )
            .to_string(),
        );
    }
    out
}

pub fn check() -> Vec<Finding> {
    evaluate(&gather())
}

pub fn has_blocker(findings: &[Finding]) -> bool {
    findings.iter().any(|f| f.level == Level::Blocker)
}

/// Self-test after installing into `dir`. With `registered`, also checks the sidebar
/// registration and instantiates every configured root through COM, as Explorer does.
pub fn verify_install(dir: &Path, registered: bool) -> Result<(), String> {
    let dll = dir.join(SHELL_DLL);
    if !dll.is_file() {
        return Err(format!(
            "{} {}",
            tr("找不到擴充套件：", "Extension not found:"),
            dll.display()
        ));
    }
    if !registered {
        return Ok(());
    }
    let expected = dll.display().to_string();
    let cfg = AppConfig::load();
    let mut problems = Vec::new();
    for root in &cfg.root_folders {
        let key = format!("Software\\Classes\\CLSID\\{}\\InprocServer32", root.clsid);
        match read_reg_sz(HKEY_CURRENT_USER, &key, None) {
            Some(p) if p.eq_ignore_ascii_case(&expected) => {}
            Some(p) => problems.push(format!(
                "{}「{}」{} {p}",
                tr("側邊欄項目", "Sidebar item"),
                root.name,
                tr("指向其他檔案：", "points to another file:")
            )),
            None => problems.push(format!(
                "{}「{}」{}",
                tr("側邊欄項目", "Sidebar item"),
                root.name,
                tr("沒有註冊。", "is not registered.")
            )),
        }
        let dll32 = crate::install::shell_dll_x86(dir);
        if dll32.is_file() {
            let expected32 = dll32.display().to_string();
            match cfd_core::registry::registered_dll_path_32(&root.clsid) {
                Some(p) if p.eq_ignore_ascii_case(&expected32) => {}
                _ => problems.push(format!(
                    "{}「{}」{}",
                    tr("側邊欄項目", "Sidebar item"),
                    root.name,
                    tr(
                        "沒有註冊給 32 位元程式（它們的開啟／另存新檔視窗會看不到）。",
                        "is not registered for 32-bit programs (their Open / Save As dialogs will not show it)."
                    )
                )),
            }
        }
        let Some(clsid) = parse_guid(&root.clsid) else {
            problems.push(format!("CLSID {}", root.clsid));
            continue;
        };
        // SAFETY: COM is initialised on this thread (main); the object is released at once.
        let created: windows::core::Result<IShellFolder> =
            unsafe { CoCreateInstance(&clsid, None, CLSCTX_INPROC_SERVER) };
        if let Err(e) = created {
            problems.push(format!(
                "{}「{}」{} 0x{:08X} {}",
                tr("無法載入側邊欄項目", "Cannot load sidebar item"),
                root.name,
                tr("：", ":"),
                e.code().0,
                e.message()
            ));
        }
    }
    // SAFETY: lets COM unload the extension DLL again (it is idle now).
    unsafe { CoFreeUnusedLibraries() };
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("\n"))
    }
}

/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` (braces optional); `None` when malformed
/// (`GUID::from(&str)` would panic on bad input).
fn parse_guid(s: &str) -> Option<GUID> {
    let s = s.trim().trim_start_matches('{').trim_end_matches('}');
    let ok = s.len() == 36
        && s.chars().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    ok.then(|| GUID::from(s))
}

/// One text block for message boxes.
pub fn report(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| {
            let tag = match f.level {
                Level::Blocker => tr("［無法安裝］", "[Blocked] "),
                Level::Warning => tr("［注意］", "[Note] "),
            };
            format!("{tag}{}", f.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(build: u32, arch: &str) -> Facts {
        Facts {
            build: Some(build),
            native_arch: Some(arch.into()),
            elevated: false,
            explorer_shell: true,
        }
    }

    #[test]
    fn guid_parsing_rejects_garbage() {
        assert!(parse_guid("{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042}").is_some());
        assert!(parse_guid("9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042").is_some());
        assert!(parse_guid("{9F3B4A10-7C2E-4D8F-A512}").is_none());
        assert!(parse_guid("zz3B4A10-7C2E-4D8F-A512-6E8C3B9D1042").is_none());
    }

    #[test]
    fn windows_11_x64_is_clean() {
        assert!(evaluate(&facts(26100, "AMD64")).is_empty());
    }

    #[test]
    fn old_windows_and_arm64_are_blocked() {
        assert!(has_blocker(&evaluate(&facts(17133, "AMD64"))));
        assert!(has_blocker(&evaluate(&facts(26100, "ARM64"))));
        assert!(has_blocker(&evaluate(&facts(26100, "x86"))));
    }

    #[test]
    fn windows_10_and_elevation_only_warn() {
        let mut f = facts(19045, "AMD64");
        f.elevated = true;
        f.explorer_shell = false;
        let r = evaluate(&f);
        assert_eq!(r.len(), 3);
        assert!(!has_blocker(&r));
    }
}
