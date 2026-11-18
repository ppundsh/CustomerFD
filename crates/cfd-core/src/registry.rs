use crate::config::{config_dir, to_wide, AppConfig, RootFolderEntry, CLSID_CUSTOMERFD_STR};
use std::path::{Path, PathBuf};
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegDeleteTreeW, RegOpenKeyExW,
    RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, SHChangeNotify, SHGetKnownFolderIDList, SHCNE_ASSOCCHANGED, SHCNE_UPDATEDIR,
    SHCNF_FLUSH, SHCNF_IDLIST,
};

pub fn write_reg_sz(
    root: HKEY,
    subkey: &str,
    name: Option<&str>,
    value: &str,
) -> Result<(), String> {
    let subkey_w = to_wide(subkey);
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("RegCreateKeyExW({subkey}) failed: {:?}", status));
    }

    let val_w = to_wide(value);
    let name_w = name.map(to_wide);
    let name_ptr = name_w
        .as_ref()
        .map(|w| PCWSTR(w.as_ptr()))
        .unwrap_or(PCWSTR::null());

    let bytes = unsafe { std::slice::from_raw_parts(val_w.as_ptr().cast::<u8>(), val_w.len() * 2) };
    let res = unsafe { RegSetValueExW(hkey, name_ptr, 0, REG_SZ, Some(bytes)) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if res != ERROR_SUCCESS {
        return Err(format!("RegSetValueExW({subkey}) failed: {:?}", res));
    }
    Ok(())
}

pub fn write_reg_dword(root: HKEY, subkey: &str, name: &str, value: u32) -> Result<(), String> {
    let subkey_w = to_wide(subkey);
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            root,
            PCWSTR(subkey_w.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("RegCreateKeyExW({subkey}) failed: {:?}", status));
    }

    let name_w = to_wide(name);
    let bytes = value.to_le_bytes();
    let res = unsafe { RegSetValueExW(hkey, PCWSTR(name_w.as_ptr()), 0, REG_DWORD, Some(&bytes)) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if res != ERROR_SUCCESS {
        return Err(format!(
            "RegSetValueExW({subkey}\\{name}) failed: {:?}",
            res
        ));
    }
    Ok(())
}

pub fn read_reg_sz(root: HKEY, subkey: &str, name: Option<&str>) -> Option<String> {
    let subkey_w = to_wide(subkey);
    let mut hkey = HKEY::default();
    let status = unsafe { RegOpenKeyExW(root, PCWSTR(subkey_w.as_ptr()), 0, KEY_READ, &mut hkey) };
    if status != ERROR_SUCCESS {
        return None;
    }
    let name_w = name.map(to_wide);
    let name_ptr = name_w
        .as_ref()
        .map(|w| PCWSTR(w.as_ptr()))
        .unwrap_or(PCWSTR::null());

    let mut buf = [0u8; 2048];
    let mut len = buf.len() as u32;
    let mut reg_type = 0u32;
    let res = unsafe {
        RegQueryValueExW(
            hkey,
            name_ptr,
            None,
            Some(&mut reg_type as *mut u32 as *mut _),
            Some(buf.as_mut_ptr()),
            Some(&mut len),
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if res != ERROR_SUCCESS || len < 2 {
        return None;
    }
    let u16_len = (len as usize) / 2;
    let slice = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u16>(), u16_len) };
    let trimmed = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    Some(String::from_utf16_lossy(&slice[..trimmed]))
}

pub fn delete_reg_tree(root: HKEY, subkey: &str) {
    let subkey_w = to_wide(subkey);
    unsafe {
        let _ = RegDeleteTreeW(root, PCWSTR(subkey_w.as_ptr()));
    }
}

pub fn delete_reg_value(root: HKEY, subkey: &str, name: &str) {
    let subkey_w = to_wide(subkey);
    let name_w = to_wide(name);
    unsafe {
        let _ = RegDeleteKeyValueW(root, PCWSTR(subkey_w.as_ptr()), PCWSTR(name_w.as_ptr()));
    }
}

pub fn find_shell_dll() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("cfd_shell.dll");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    let cwd_candidates = [
        PathBuf::from("cfd_shell.dll"),
        PathBuf::from("target/release/cfd_shell.dll"),
        PathBuf::from("target/debug/cfd_shell.dll"),
    ];
    cwd_candidates.into_iter().find(|p| p.exists())
}

pub fn is_registered() -> bool {
    let cfg = AppConfig::load();
    cfg.root_folders
        .iter()
        .any(|r| is_clsid_registered(&r.clsid))
}

fn is_clsid_registered(clsid: &str) -> bool {
    let clsid_key = format!("Software\\Classes\\CLSID\\{clsid}\\InprocServer32");
    let ns_key = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Desktop\\NameSpace\\{clsid}"
    );
    let dll_ok = read_reg_sz(HKEY_CURRENT_USER, &clsid_key, None)
        .map(|p| Path::new(&p).exists())
        .unwrap_or(false);
    let ns_ok = read_reg_sz(HKEY_CURRENT_USER, &ns_key, None).is_some();
    dll_ok && ns_ok
}

pub fn registered_dll_path() -> Option<String> {
    let clsid_key = format!("Software\\Classes\\CLSID\\{CLSID_CUSTOMERFD_STR}\\InprocServer32");
    read_reg_sz(HKEY_CURRENT_USER, &clsid_key, None)
}

fn register_single_root(
    root: &RootFolderEntry,
    show_in_sidebar: bool,
    hide_from_desktop: bool,
    dll_str: &str,
) -> Result<(), String> {
    let clsid = &root.clsid;
    let clsid_base = format!("Software\\Classes\\CLSID\\{clsid}");

    write_reg_sz(HKEY_CURRENT_USER, &clsid_base, None, &root.name)?;
    write_reg_dword(
        HKEY_CURRENT_USER,
        &clsid_base,
        "System.IsPinnedToNameSpaceTree",
        if show_in_sidebar { 1 } else { 0 },
    )?;
    write_reg_dword(
        HKEY_CURRENT_USER,
        &clsid_base,
        "SortOrderIndex",
        root.sort_order_index,
    )?;

    let icon_key = format!("{clsid_base}\\DefaultIcon");
    let icon_val = if root.icon.trim().is_empty() {
        "shell32.dll,-4"
    } else {
        root.icon.trim()
    };
    write_reg_sz(HKEY_CURRENT_USER, &icon_key, None, icon_val)?;

    let inproc_key = format!("{clsid_base}\\InprocServer32");
    write_reg_sz(HKEY_CURRENT_USER, &inproc_key, None, dll_str)?;
    write_reg_sz(
        HKEY_CURRENT_USER,
        &inproc_key,
        Some("ThreadingModel"),
        "Apartment",
    )?;

    let sf_key = format!("{clsid_base}\\ShellFolder");
    write_reg_dword(HKEY_CURRENT_USER, &sf_key, "Attributes", 0xA800_0000)?;
    write_reg_dword(HKEY_CURRENT_USER, &sf_key, "FolderValueFlags", 0x28)?;

    let ns_key = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Desktop\\NameSpace\\{clsid}"
    );
    write_reg_sz(HKEY_CURRENT_USER, &ns_key, None, &root.name)?;

    let hide_key =
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\HideDesktopIcons\\NewStartPanel";
    if hide_from_desktop {
        write_reg_dword(HKEY_CURRENT_USER, hide_key, clsid, 1)?;
    } else {
        delete_reg_value(HKEY_CURRENT_USER, hide_key, clsid);
    }
    Ok(())
}

pub fn unregister_single_clsid(clsid: &str) {
    let clsid_base = format!("Software\\Classes\\CLSID\\{clsid}");
    let ns_key = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Desktop\\NameSpace\\{clsid}"
    );
    let hide_key =
        "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\HideDesktopIcons\\NewStartPanel";

    delete_reg_tree(HKEY_CURRENT_USER, &ns_key);
    delete_reg_tree(HKEY_CURRENT_USER, &clsid_base);
    delete_reg_value(HKEY_CURRENT_USER, hide_key, clsid);
}

/// Registers all configured root folders in HKCU (no admin required) and cleans up
/// any stale CustomerFD CLSIDs that were removed.
pub fn register_namespace(cfg: &AppConfig, dll_path: &Path) -> Result<(), String> {
    let dll_str = dll_path
        .to_str()
        .ok_or_else(|| "Invalid DLL path encoding".to_string())?;

    // Remove stale CLSIDs in our reserved range.
    for clsid in crate::config::reserved_clsids() {
        if !cfg
            .root_folders
            .iter()
            .any(|r| r.clsid.eq_ignore_ascii_case(&clsid))
        {
            unregister_single_clsid(&clsid);
        }
    }

    for root in &cfg.root_folders {
        register_single_root(root, cfg.show_in_sidebar, cfg.hide_from_desktop, dll_str)?;
    }

    notify_explorer_refresh();
    Ok(())
}

pub fn unregister_namespace() {
    let cfg = AppConfig::load();
    for root in &cfg.root_folders {
        unregister_single_clsid(&root.clsid);
    }
    for clsid in crate::config::reserved_clsids() {
        unregister_single_clsid(&clsid);
    }
    notify_explorer_refresh();
}

/// Removes all registry entries AND deletes `%LOCALAPPDATA%\CustomerFD` (config & custom icons).
pub fn cleanup_all() -> Result<(), String> {
    unregister_namespace();
    let dir = config_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)
            .map_err(|e| format!("Failed to remove config directory: {e}"))?;
    }
    notify_explorer_refresh();
    Ok(())
}

pub fn notify_explorer_refresh() {
    unsafe {
        if let Ok(desktop_pidl) = SHGetKnownFolderIDList(&FOLDERID_Desktop, 0, None) {
            SHChangeNotify(
                SHCNE_UPDATEDIR,
                SHCNF_IDLIST | SHCNF_FLUSH,
                Some(desktop_pidl as *const _),
                None,
            );
            CoTaskMemFree(Some(desktop_pidl as *const _));
        }
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

pub fn restart_explorer() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let explorer_running = || {
        Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq explorer.exe", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .to_ascii_lowercase()
                    .contains("explorer.exe")
            })
            .unwrap_or(false)
    };
    let _ = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    // Windows usually restarts the shell by itself (AutoRestartShell); only start it when it
    // has not come back, otherwise a stray File Explorer window would open.
    for _ in 0..15 {
        std::thread::sleep(std::time::Duration::from_millis(200));
        if explorer_running() {
            return Ok(());
        }
    }
    Command::new("explorer.exe")
        .spawn()
        .map_err(|e| format!("Failed to launch explorer.exe: {e}"))?;
    Ok(())
}
