//! Shell-extension diagnostic trace switch.
//!
//! The switch lives machine-wide in `HKLM\Software\CustomerFD` (`ShellTrace` = 1), so only an
//! administrator can turn it on or off; any process (including explorer.exe at medium
//! integrity) can read it. The control center flips it by re-launching itself elevated with
//! [`SET_TRACE_ARG`]. The shell extension reads it once per process, so a change takes effect
//! after Explorer restarts.

use crate::config::{config_dir, to_wide};
use std::path::PathBuf;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteKeyValueW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_LOCAL_MACHINE,
    KEY_READ, KEY_WOW64_64KEY,
};

/// Registry key (under HKLM) holding the switch.
pub const TRACE_KEY: &str = r"Software\CustomerFD";
/// DWORD value: 1 = trace on; absent or 0 = off.
pub const TRACE_VALUE: &str = "ShellTrace";
/// Command-line switch of the control center, run elevated: `--set-shell-trace on|off`.
pub const SET_TRACE_ARG: &str = "--set-shell-trace";

/// Whether the shell extension should write its diagnostic trace.
pub fn shell_trace_enabled() -> bool {
    let key_w = to_wide(TRACE_KEY);
    let value_w = to_wide(TRACE_VALUE);
    let mut hkey = HKEY::default();
    unsafe {
        if RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(key_w.as_ptr()),
            0,
            KEY_READ | KEY_WOW64_64KEY,
            &mut hkey,
        ) != ERROR_SUCCESS
        {
            return false;
        }
        let mut data = 0u32;
        let mut len = std::mem::size_of::<u32>() as u32;
        let res = RegQueryValueExW(
            hkey,
            PCWSTR(value_w.as_ptr()),
            None,
            None,
            Some((&mut data as *mut u32).cast()),
            Some(&mut len),
        );
        let _ = RegCloseKey(hkey);
        res == ERROR_SUCCESS && len == 4 && data != 0
    }
}

/// Turns the trace on or off. Needs administrator rights (writes HKLM).
pub fn set_shell_trace(on: bool) -> Result<(), String> {
    if on {
        crate::registry::write_reg_dword(HKEY_LOCAL_MACHINE, TRACE_KEY, TRACE_VALUE, 1)?;
    } else {
        let key_w = to_wide(TRACE_KEY);
        let value_w = to_wide(TRACE_VALUE);
        let res = unsafe {
            RegDeleteKeyValueW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key_w.as_ptr()),
                PCWSTR(value_w.as_ptr()),
            )
        };
        if res != ERROR_SUCCESS && res != ERROR_FILE_NOT_FOUND {
            return Err(format!("RegDeleteKeyValueW failed: {res:?}"));
        }
    }
    if shell_trace_enabled() == on {
        Ok(())
    } else {
        Err("the setting did not change".into())
    }
}

/// Where the shell extension writes its trace (per user).
pub fn shell_trace_log_path() -> PathBuf {
    config_dir().join("shell-trace.log")
}

/// Deletes the trace logs (main and fallback). Explorer keeps writing until it restarts with
/// the switch off, so callers also run this after a restart and at start-up while it is off.
pub fn remove_shell_trace_logs() {
    for p in std::iter::once(shell_trace_log_path()).chain(shell_trace_fallback_path()) {
        let _ = std::fs::remove_file(p);
    }
}

/// Fallback location when [`shell_trace_log_path`] is not writable from the host process.
pub fn shell_trace_fallback_path() -> Option<PathBuf> {
    std::env::var_os("TEMP").map(|t| PathBuf::from(t).join("cfd-shell-trace.log"))
}
