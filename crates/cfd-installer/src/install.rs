//! Install, in-place update and uninstall (per user, HKCU only, no UAC).

use cfd_core::registry::{
    delete_reg_tree, notify_explorer_refresh, read_reg_sz, unregister_namespace, write_reg_dword,
    write_reg_sz,
};
use cfd_ui::wstr::to_wide;
use std::path::{Path, PathBuf};
use windows::core::{Interface, PCSTR, PCWSTR};
use windows::Win32::Foundation::{FreeLibrary, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::System::Registry::HKEY_CURRENT_USER;
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

pub const APP_NAME: &str = "CustomerFD";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const UNINSTALL_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CustomerFD";
const MANAGER_EXE: &str = "cfd-manager.exe";
const SHELL_DLL: &str = "cfd_shell.dll";
const UNINSTALLER_EXE: &str = "uninstall.exe";
const MANAGER_CLASS: &str = "CustomerFDManagerWindow";

static PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

/// Files embedded in the installer: `(name, bytes)`.
pub fn payload() -> Result<Vec<(String, &'static [u8])>, String> {
    let d = PAYLOAD;
    if d.len() < 12 || &d[..8] != b"CFDPAK1\0" {
        return Err("corrupt installer payload".into());
    }
    let count = u32::from_le_bytes(d[8..12].try_into().unwrap()) as usize;
    let mut pos = 12;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let take = |pos: &mut usize, n: usize| -> Result<&'static [u8], String> {
            let s = d.get(*pos..*pos + n).ok_or("truncated installer payload")?;
            *pos += n;
            Ok(s)
        };
        let nl = u16::from_le_bytes(take(&mut pos, 2)?.try_into().unwrap()) as usize;
        let name =
            String::from_utf8(take(&mut pos, nl)?.to_vec()).map_err(|_| "bad payload name")?;
        if name.contains(['/', '\\']) || name.contains("..") {
            return Err("bad payload name".into());
        }
        let sz = u64::from_le_bytes(take(&mut pos, 8)?.try_into().unwrap()) as usize;
        out.push((name, take(&mut pos, sz)?));
    }
    if out.is_empty() {
        return Err(crate::tr(
            "這個安裝程式沒有包含程式檔（請用 build.ps1 建置）。",
            "This installer contains no program files (build it with build.ps1).",
        )
        .into());
    }
    Ok(out)
}

pub fn default_install_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("C:\\"));
    base.join("Programs").join(APP_NAME)
}

/// Folder of an existing installation (from the uninstall entry).
pub fn installed_dir() -> Option<PathBuf> {
    read_reg_sz(HKEY_CURRENT_USER, UNINSTALL_KEY, Some("InstallLocation"))
        .map(PathBuf::from)
        .filter(|p| p.join(MANAGER_EXE).exists())
}

pub fn installed_version() -> Option<String> {
    read_reg_sz(HKEY_CURRENT_USER, UNINSTALL_KEY, Some("DisplayVersion"))
}

fn start_menu_lnk() -> Option<PathBuf> {
    let appdata = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(appdata)
            .join("Microsoft\\Windows\\Start Menu\\Programs")
            .join(format!("{APP_NAME}.lnk")),
    )
}

/// Writes `bytes` to `path`; a locked file (a DLL loaded by Explorer, a running exe) is renamed
/// to `*.N.old` first, which Windows allows, and removed on a later update.
fn write_replacing(path: &Path, bytes: &[u8]) -> Result<(), String> {
    match std::fs::write(path, bytes) {
        Ok(()) => Ok(()),
        Err(first) => {
            if !path.exists() {
                return Err(format!("{}: {first}", path.display()));
            }
            let mut n = 1;
            let old = loop {
                let cand = path.with_extension(format!(
                    "{}.{n}.old",
                    path.extension().and_then(|e| e.to_str()).unwrap_or("")
                ));
                if !cand.exists() {
                    break cand;
                }
                n += 1;
            };
            std::fs::rename(path, &old)
                .map_err(|e| format!("{}: {first} / {e}", path.display()))?;
            std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))
        }
    }
}

fn remove_stale_old_files(dir: &Path) {
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("old") {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

/// Calls `DllRegisterServer` / `DllUnregisterServer` of the installed shell extension.
fn call_dll_export(dll: &Path, export: &str) -> Result<(), String> {
    let w = to_wide(&dll.display().to_string());
    // SAFETY: loading our own DLL and calling an export with the documented signature.
    unsafe {
        let h = LoadLibraryW(PCWSTR(w.as_ptr())).map_err(|e| format!("{}: {e}", dll.display()))?;
        let name = format!("{export}\0");
        let f = GetProcAddress(h, PCSTR(name.as_ptr()));
        let r = match f {
            Some(f) => {
                let f: extern "system" fn() -> windows::core::HRESULT = std::mem::transmute(f);
                let hr = f();
                if hr.is_ok() {
                    Ok(())
                } else {
                    Err(format!("{export}: 0x{:08X}", hr.0))
                }
            }
            None => Err(format!("{export} not found")),
        };
        let _ = FreeLibrary(h);
        r
    }
}

fn create_shortcut(lnk: &Path, target: &Path, dir: &Path) -> Result<(), String> {
    // SAFETY: standard IShellLink / IPersistFile usage on an STA thread.
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let t = to_wide(&target.display().to_string());
        let d = to_wide(&dir.display().to_string());
        let desc = to_wide("CustomerFD");
        link.SetPath(PCWSTR(t.as_ptr()))
            .map_err(|e| e.to_string())?;
        link.SetWorkingDirectory(PCWSTR(d.as_ptr()))
            .map_err(|e| e.to_string())?;
        link.SetDescription(PCWSTR(desc.as_ptr()))
            .map_err(|e| e.to_string())?;
        link.SetIconLocation(PCWSTR(t.as_ptr()), 0)
            .map_err(|e| e.to_string())?;
        let pf: IPersistFile = link.cast().map_err(|e| e.to_string())?;
        if let Some(parent) = lnk.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let l = to_wide(&lnk.display().to_string());
        pf.Save(PCWSTR(l.as_ptr()), true).map_err(|e| e.to_string())
    }
}

fn dir_size_kb(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum::<u64>()
                / 1024
        })
        .unwrap_or(0) as u32
}

fn today_yyyymmdd() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // civil-from-days (Howard Hinnant)
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}")
}

fn write_uninstall_entry(dir: &Path) -> Result<(), String> {
    let k = UNINSTALL_KEY;
    let h = HKEY_CURRENT_USER;
    let exe = dir.join(MANAGER_EXE);
    let un = dir.join(UNINSTALLER_EXE);
    write_reg_sz(h, k, Some("DisplayName"), APP_NAME)?;
    write_reg_sz(h, k, Some("DisplayVersion"), VERSION)?;
    write_reg_sz(h, k, Some("Publisher"), "ppundsh")?;
    write_reg_sz(h, k, Some("InstallLocation"), &dir.display().to_string())?;
    write_reg_sz(h, k, Some("DisplayIcon"), &format!("{},0", exe.display()))?;
    write_reg_sz(
        h,
        k,
        Some("UninstallString"),
        &format!("\"{}\" --uninstall", un.display()),
    )?;
    write_reg_sz(
        h,
        k,
        Some("QuietUninstallString"),
        &format!("\"{}\" --uninstall --quiet", un.display()),
    )?;
    write_reg_sz(
        h,
        k,
        Some("URLInfoAbout"),
        "https://github.com/ppundsh/CustomerFD",
    )?;
    write_reg_sz(
        h,
        k,
        Some("HelpLink"),
        "https://github.com/ppundsh/CustomerFD/issues",
    )?;
    write_reg_sz(h, k, Some("InstallDate"), &today_yyyymmdd())?;
    write_reg_dword(h, k, "NoModify", 1)?;
    write_reg_dword(h, k, "NoRepair", 1)?;
    write_reg_dword(h, k, "EstimatedSize", dir_size_kb(dir))?;
    Ok(())
}

#[derive(Clone, Copy)]
pub struct Options {
    pub shortcut: bool,
    pub register: bool,
}

/// Installs (or updates) into `dir`. With `update`, settings are kept and the sidebar is
/// re-registered only if it was registered before.
pub fn install(dir: &Path, opts: Options, update: bool) -> Result<(), String> {
    let files = payload()?;
    let was_registered = cfd_core::registry::is_registered();
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    remove_stale_old_files(dir);
    for (name, bytes) in &files {
        write_replacing(&dir.join(name), bytes)?;
    }
    // The uninstaller is this very program.
    if let Ok(me) = std::env::current_exe() {
        let target = dir.join(UNINSTALLER_EXE);
        if !same_file(&me, &target) {
            let bytes = std::fs::read(&me).map_err(|e| e.to_string())?;
            write_replacing(&target, &bytes)?;
        }
    }
    write_uninstall_entry(dir)?;
    if opts.shortcut {
        if let Some(lnk) = start_menu_lnk() {
            let _ = create_shortcut(&lnk, &dir.join(MANAGER_EXE), dir);
        }
    }
    let register = if update {
        was_registered
    } else {
        opts.register
    };
    if register {
        call_dll_export(&dir.join(SHELL_DLL), "DllRegisterServer")?;
    }
    notify_explorer_refresh();
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Waits (up to 30 s) for the process that started an update to exit.
pub fn wait_for_pid(pid: u32) {
    // SAFETY: wait on a process handle we open with SYNCHRONIZE only.
    unsafe {
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let _ = WaitForSingleObject(h, 30_000);
            let _ = windows::Win32::Foundation::CloseHandle(h);
        }
    }
}

/// Asks a running control center to close and waits briefly.
pub fn close_running_manager() {
    let class = to_wide(MANAGER_CLASS);
    for _ in 0..20 {
        // SAFETY: window lookup / message post by class name.
        let h = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) };
        match h {
            Ok(h) if !h.is_invalid() => unsafe {
                let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            _ => return,
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
    }
}

pub fn launch_manager(dir: &Path, updated: bool) {
    let mut cmd = std::process::Command::new(dir.join(MANAGER_EXE));
    if updated {
        cmd.arg("--updated");
    }
    let _ = cmd.current_dir(dir).spawn();
}

/// Removes the program, its registry entries and (with `purge`) the user's settings.
pub fn uninstall(purge: bool) -> Result<(), String> {
    let dir = installed_dir()
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(PathBuf::from))
        })
        .ok_or("install folder not found")?;
    close_running_manager();
    unregister_namespace();
    if purge {
        let cfg = cfd_core::config::config_dir();
        if cfg.exists() {
            let _ = std::fs::remove_dir_all(cfg);
        }
    }
    // Explorer may still have the DLL loaded: move it out of the folder (same volume) instead.
    let trash = std::env::temp_dir().join(format!("CustomerFD-uninstall-{}", std::process::id()));
    for name in [MANAGER_EXE, SHELL_DLL, "LICENSE.txt", "README.md"] {
        let p = dir.join(name);
        if p.exists() && std::fs::remove_file(&p).is_err() {
            let _ = std::fs::create_dir_all(&trash);
            let _ = std::fs::rename(&p, trash.join(name));
        }
    }
    remove_stale_old_files(&dir);
    if let Some(lnk) = start_menu_lnk() {
        let _ = std::fs::remove_file(lnk);
    }
    delete_reg_tree(HKEY_CURRENT_USER, UNINSTALL_KEY);
    notify_explorer_refresh();
    // The running uninstaller cannot delete itself: let a hidden cmd finish the job.
    let me = std::env::current_exe().ok();
    if let Some(me) = me.filter(|m| m.parent() == Some(dir.as_path())) {
        use std::os::windows::process::CommandExt;
        let script = format!(
            "ping 127.0.0.1 -n 3 >nul & del /f /q \"{}\" & rmdir \"{}\"",
            me.display(),
            dir.display()
        );
        let _ = std::process::Command::new("cmd.exe")
            .args(["/c", &script])
            .creation_flags(0x0800_0000)
            .spawn();
    } else {
        let _ = std::fs::remove_dir(&dir);
    }
    Ok(())
}
