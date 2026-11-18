use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use windows::core::GUID;

/// Base CLSID for the first CustomerFD Shell Namespace Extension root folder:
/// {9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042}
pub const BASE_CLSID_U128: u128 = 0x9f3b4a10_7c2e_4d8f_a512_6e8c3b9d1042;
pub const CLSID_CUSTOMERFD_STR: &str = "{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042}";
/// Number of CLSIDs reserved from [`BASE_CLSID_U128`] (`uninstall.ps1` and the installer clean
/// exactly this range), i.e. the maximum number of root folders.
pub const MAX_ROOT_FOLDERS: u128 = 64;

/// All reserved root-folder CLSIDs, formatted `{XXXXXXXX-...}`.
pub fn reserved_clsids() -> impl Iterator<Item = String> {
    (0..MAX_ROOT_FOLDERS).map(|i| format_guid(&GUID::from_u128(BASE_CLSID_U128 + i)))
}

pub fn format_guid(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

pub fn parse_guid(s: &str) -> Option<GUID> {
    let hex: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() != 32 {
        return None;
    }
    let val = u128::from_str_radix(&hex, 16).ok()?;
    Some(GUID::from_u128(val))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiLang {
    ZhTw,
    En,
}

impl Default for UiLang {
    fn default() -> Self {
        Self::ZhTw
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RootFolderEntry {
    pub id: String,
    pub clsid: String,
    pub name: String,
    #[serde(default)]
    pub target_path: String,
    #[serde(default = "default_root_icon")]
    pub icon: String,
    #[serde(default = "default_sort_order")]
    pub sort_order_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubFolderEntry {
    pub id: String,
    #[serde(default = "default_root_id")]
    pub root_id: String,
    pub name: String,
    pub target_path: String,
    #[serde(default)]
    pub icon_path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

fn default_root_id() -> String {
    "root_1".to_string()
}

fn default_root_name() -> String {
    "自訂資料夾".to_string()
}

fn default_root_icon() -> String {
    "shell32.dll,-4".to_string()
}

fn default_sort_order() -> u32 {
    0x42
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_true")]
    pub show_in_sidebar: bool,
    #[serde(default = "default_true")]
    pub hide_from_desktop: bool,
    #[serde(default)]
    pub ui_lang: UiLang,
    #[serde(default)]
    pub root_folders: Vec<RootFolderEntry>,
    #[serde(default)]
    pub subfolders: Vec<SubFolderEntry>,
    /// Check GitHub Releases for a newer version at most once a day (notify only).
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    /// Unix seconds of the last successful update check (0 = never).
    #[serde(default)]
    pub last_update_check: u64,
    /// Navigation rail collapsed to icons.
    #[serde(default)]
    pub nav_collapsed: bool,
    // Legacy fields for seamless migration from v0.1.0 single-root config
    #[serde(default, skip_serializing)]
    pub root_name: Option<String>,
    #[serde(default, skip_serializing)]
    pub root_icon: Option<String>,
    #[serde(default, skip_serializing)]
    pub sort_order_index: Option<u32>,
}

impl Default for AppConfig {
    fn default() -> Self {
        let user_profile = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users".to_string());
        let downloads = format!("{}\\Downloads", user_profile);
        let documents = format!("{}\\Documents", user_profile);
        Self {
            show_in_sidebar: true,
            hide_from_desktop: true,
            ui_lang: UiLang::ZhTw,
            root_folders: vec![RootFolderEntry {
                id: default_root_id(),
                clsid: CLSID_CUSTOMERFD_STR.to_string(),
                name: default_root_name(),
                target_path: String::new(),
                icon: default_root_icon(),
                sort_order_index: default_sort_order(),
            }],
            subfolders: vec![
                SubFolderEntry {
                    id: "sub_1".to_string(),
                    root_id: default_root_id(),
                    name: "下載項目".to_string(),
                    target_path: downloads,
                    icon_path: String::new(),
                    enabled: true,
                },
                SubFolderEntry {
                    id: "sub_2".to_string(),
                    root_id: default_root_id(),
                    name: "文件資料".to_string(),
                    target_path: documents,
                    icon_path: String::new(),
                    enabled: true,
                },
            ],
            auto_check_updates: true,
            last_update_check: 0,
            nav_collapsed: false,
            root_name: None,
            root_icon: None,
            sort_order_index: None,
        }
    }
}

pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .or_else(|_| std::env::var("APPDATA"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("CustomerFD")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

impl AppConfig {
    pub fn load() -> Self {
        let path = config_path();
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(mut cfg) = serde_json::from_str::<AppConfig>(&text) {
                if cfg.root_folders.is_empty() {
                    cfg.root_folders.push(RootFolderEntry {
                        id: default_root_id(),
                        clsid: CLSID_CUSTOMERFD_STR.to_string(),
                        name: cfg
                            .root_name
                            .take()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(default_root_name),
                        target_path: String::new(),
                        icon: cfg
                            .root_icon
                            .take()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(default_root_icon),
                        sort_order_index: cfg.sort_order_index.take().unwrap_or_else(default_sort_order),
                    });
                }
                for sub in &mut cfg.subfolders {
                    if sub.root_id.trim().is_empty() {
                        sub.root_id = cfg.root_folders[0].id.clone();
                    }
                }
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = config_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {e}"))?;
        let path = config_path();
        let tmp_path = dir.join("config.json.tmp");
        std::fs::write(&tmp_path, json).map_err(|e| format!("Failed to write temp config: {e}"))?;
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        std::fs::rename(&tmp_path, &path)
            .map_err(|e| format!("Failed to replace config file: {e}"))?;
        Ok(())
    }

    pub fn next_subfolder_id(&self) -> String {
        let mut n = self.subfolders.len() + 1;
        loop {
            let candidate = format!("sub_{n}");
            if !self.subfolders.iter().any(|s| s.id == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Next free root folder, or `None` when all [`MAX_ROOT_FOLDERS`] CLSIDs are in use.
    pub fn next_root_folder(&self) -> Option<RootFolderEntry> {
        if self.root_folders.len() as u128 >= MAX_ROOT_FOLDERS {
            return None;
        }
        let mut n = self.root_folders.len() + 1;
        let id = loop {
            let candidate = format!("root_{n}");
            if !self.root_folders.iter().any(|r| r.id == candidate) {
                break candidate;
            }
            n += 1;
        };
        let clsid = reserved_clsids().find(|s| {
            !self
                .root_folders
                .iter()
                .any(|r| r.clsid.eq_ignore_ascii_case(s))
        })?;
        Some(RootFolderEntry {
            id,
            clsid,
            name: format!("自訂資料夾 {n}"),
            target_path: String::new(),
            icon: default_root_icon(),
            sort_order_index: default_sort_order() + (self.root_folders.len() as u32),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_range_matches_uninstall_script() {
        let all: Vec<String> = reserved_clsids().collect();
        assert_eq!(all.len(), 64);
        assert_eq!(all[0], CLSID_CUSTOMERFD_STR);
        assert_eq!(all[63], "{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1081}");
    }

    #[test]
    fn root_folders_are_capped() {
        let mut cfg = AppConfig::default();
        while let Some(r) = cfg.next_root_folder() {
            cfg.root_folders.push(r);
        }
        assert_eq!(cfg.root_folders.len() as u128, MAX_ROOT_FOLDERS);
        let mut seen = std::collections::HashSet::new();
        assert!(cfg.root_folders.iter().all(|r| seen.insert(r.clsid.to_ascii_uppercase())));
    }

    #[test]
    fn old_config_without_new_fields_loads() {
        let json = r#"{"show_in_sidebar":true,"hide_from_desktop":false,"ui_lang":"En","root_folders":[],"subfolders":[]}"#;
        let cfg: AppConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.auto_check_updates);
        assert_eq!(cfg.last_update_check, 0);
        assert!(!cfg.nav_collapsed);
    }
}
