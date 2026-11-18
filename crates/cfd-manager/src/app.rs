//! Control-center state, hit regions and actions.

use crate::i18n::{set_lang, tr};
use crate::sys;
use crate::updater;
use cfd_core::config::{config_dir, AppConfig, SubFolderEntry, UiLang};
use cfd_core::custom_icon::{delete_custom_icon, import_image_as_ico, list_custom_icons};
use cfd_core::registry::{
    cleanup_all, find_shell_dll, is_registered, notify_explorer_refresh, register_namespace,
    restart_explorer, unregister_namespace,
};
use cfd_ui::anim::{ease_out_cubic, Tween};
use cfd_ui::gdi::contains;
use cfd_ui::widgets::Severity;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, HICON, KillTimer, SetTimer};

pub const TIMER_ANIM: usize = 1;
pub const TIMER_CARET: usize = 2;
pub const TIMER_INFO: usize = 3;

pub const DEFAULT_ROOT_ICON: &str = "shell32.dll,-4";

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Page {
    Sidebar,
    Subfolders,
    About,
}

impl Page {
    pub const ALL: [Page; 3] = [Page::Sidebar, Page::Subfolders, Page::About];
    pub fn index(self) -> usize {
        match self {
            Page::Sidebar => 0,
            Page::Subfolders => 1,
            Page::About => 2,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Page::Sidebar => tr("側邊欄設定", "Sidebar"),
            Page::Subfolders => tr("子資料夾", "Subfolders"),
            Page::About => tr("關於", "About"),
        }
    }
    pub fn glyph(self) -> &'static str {
        match self {
            Page::Sidebar => cfd_ui::widgets::GLYPH_FOLDER,
            Page::Subfolders => cfd_ui::widgets::GLYPH_FOLDER_OPEN,
            Page::About => cfd_ui::widgets::GLYPH_INFO,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Field {
    RootName,
    RootPath,
    RootIcon,
    SubName,
    SubPath,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ToggleId {
    ShowInSidebar,
    HideDesktop,
    AutoUpdate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ExpanderId {
    Icon,
    SubEditor,
}

/// Something that needs the user's confirmation in a modal dialog.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Confirm {
    DeleteRoot(usize),
    DeleteSub(usize),
    DeleteIcon(PathBuf),
    Unregister,
    RestartExplorer,
    Cleanup,
    InstallUpdate,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    /// Swallows clicks (modal backdrop / card body).
    Block,
    NavToggle,
    SwitchPage(Page),
    LangButton,
    SetLang(UiLang),
    FocusField(Field),
    ClearField(Field),
    SelectRoot(usize),
    AddRoot,
    Ask(Confirm),
    Toggle(ToggleId),
    Expander(ExpanderId),
    PreviewToggle,
    Apply,
    BrowseIcon,
    BrowseRootPath,
    ImportIcon,
    SelectIcon(String),
    BrowseSubPath,
    SaveSub,
    CancelSub,
    SubEnabled(String),
    SubUp(usize),
    SubDown(usize),
    EditSub(usize),
    OpenSub(usize),
    OpenConfigDir,
    OpenInstallDir,
    CheckUpdate,
    ReleaseNotes,
    OpenGitHub,
    OpenLicense,
    DismissInfo,
    ModalConfirm,
    ModalCancel,
    ScrollThumb,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Arrow,
    Hand,
    IBeam,
}

#[derive(Clone)]
pub struct Region {
    pub rect: RECT,
    pub action: Action,
    pub cursor: Cursor,
}

/// Hover / press state of a widget for the current frame.
#[derive(Clone, Copy, Default)]
pub struct Hot {
    pub hovered: bool,
    pub pressed: bool,
}

#[derive(Clone, Default)]
pub struct EditState {
    pub caret: usize,
    pub anchor: Option<usize>,
    pub dragging: bool,
    /// Text box rectangle from the last frame (for mouse → caret mapping).
    pub box_rect: RECT,
    /// Horizontal scroll (px) so the caret stays visible.
    pub scroll_x: i32,
}

pub struct ExpanderState {
    pub expanded: bool,
    pub tween: Tween,
    pub press: Tween,
}

impl ExpanderState {
    fn new(expanded: bool) -> Self {
        Self { expanded, tween: Tween::new(180), press: Tween::new(140) }
    }
    /// 0 = collapsed, 1 = expanded (eased).
    pub fn openness(&self) -> f32 {
        let p = if self.tween.is_active() { ease_out_cubic(self.tween.raw()) } else { 1.0 };
        if self.expanded {
            p
        } else {
            1.0 - p
        }
    }
}

pub struct ModalState {
    pub confirm: Confirm,
    pub title: String,
    pub body: String,
    pub ok_label: String,
    pub danger: bool,
}

pub struct InfoState {
    pub severity: Severity,
    pub message: String,
}

/// Cached shell icons keyed by (location, size); destroyed on drop / clear.
#[derive(Default)]
pub struct IconCache {
    map: HashMap<(String, i32), Option<HICON>>,
}

impl IconCache {
    pub fn get(&mut self, loc: &str, size: i32) -> Option<HICON> {
        *self
            .map
            .entry((loc.to_string(), size))
            // SAFETY: shell icon extraction; the handle is owned by the cache.
            .or_insert_with(|| unsafe { sys::load_shell_icon(loc, size) })
    }
    pub fn clear(&mut self) {
        for (_, h) in self.map.drain() {
            if let Some(h) = h {
                // SAFETY: icons were created by SHDefExtractIconW and are owned by the cache.
                unsafe {
                    let _ = DestroyIcon(h);
                }
            }
        }
    }
}

impl Drop for IconCache {
    fn drop(&mut self) {
        self.clear();
    }
}

pub struct AppState {
    pub hwnd: HWND,
    pub dpi: u32,
    pub page: Page,
    pub config: AppConfig,
    /// Root-folder settings as last written to the registry ("" = never applied).
    pub applied_snapshot: String,
    pub selected_root: usize,
    pub registered: bool,
    pub preview_expanded: bool,
    pub editing_sub: Option<String>,
    pub sub_name: String,
    pub sub_path: String,
    pub focused: Option<Field>,
    pub edits: HashMap<Field, EditState>,
    pub caret_visible: bool,
    pub custom_icons: Vec<PathBuf>,
    pub icon_previews: HashMap<(PathBuf, u32), Option<Vec<u8>>>,
    pub icons: IconCache,
    pub info: Option<InfoState>,
    pub modal: Option<ModalState>,
    // layout + interaction
    pub regions: Vec<Region>,
    pub mouse: (i32, i32),
    pub mouse_inside: bool,
    pub pressed: Option<Action>,
    pub hover_action: Option<Action>,
    pub block_input: bool,
    pub scroll_y: i32,
    pub max_scroll: i32,
    pub view_h: i32,
    pub scroll_track: RECT,
    pub thumb_drag: Option<(i32, i32)>,
    // animations
    pub nav_collapsed: bool,
    pub nav_tween: Tween,
    pub indicator_from: usize,
    pub indicator_tween: Tween,
    pub nav_press: Tween,
    pub page_tween: Tween,
    pub lang_open: bool,
    pub lang_tween: Tween,
    pub lang_press: Tween,
    pub toggles: HashMap<ToggleId, f32>,
    pub checks: HashMap<String, Tween>,
    pub expanders: HashMap<ExpanderId, ExpanderState>,
    // update
    pub update_auto_started: bool,
}

thread_local! {
    pub static STATE: RefCell<Option<AppState>> = const { RefCell::new(None) };
}

/// Runs `f` on the state unless it is already borrowed (re-entrant message during a modal
/// loop); returns `None` in that case so the caller can simply skip or repaint later.
pub fn with_state<R>(f: impl FnOnce(&mut AppState) -> R) -> Option<R> {
    STATE.with(|cell| {
        let mut guard = cell.try_borrow_mut().ok()?;
        guard.as_mut().map(f)
    })
}

pub fn root_snapshot(cfg: &AppConfig) -> String {
    serde_json::to_string(&(&cfg.root_folders, cfg.show_in_sidebar, cfg.hide_from_desktop)).unwrap_or_default()
}

impl AppState {
    pub fn new(hwnd: HWND, dpi: u32, just_updated: bool) -> Self {
        let config = AppConfig::load();
        set_lang(config.ui_lang);
        let registered = is_registered();
        let applied_snapshot = if registered { root_snapshot(&config) } else { String::new() };
        let mut toggles = HashMap::new();
        toggles.insert(ToggleId::ShowInSidebar, config.show_in_sidebar as u8 as f32);
        toggles.insert(ToggleId::HideDesktop, config.hide_from_desktop as u8 as f32);
        toggles.insert(ToggleId::AutoUpdate, config.auto_check_updates as u8 as f32);
        let mut expanders = HashMap::new();
        expanders.insert(ExpanderId::Icon, ExpanderState::new(false));
        expanders.insert(ExpanderId::SubEditor, ExpanderState::new(true));
        let nav_collapsed = config.nav_collapsed;
        let mut st = Self {
            hwnd,
            dpi,
            page: Page::Sidebar,
            config,
            applied_snapshot,
            selected_root: 0,
            registered,
            preview_expanded: true,
            editing_sub: None,
            sub_name: String::new(),
            sub_path: String::new(),
            focused: None,
            edits: HashMap::new(),
            caret_visible: true,
            custom_icons: list_custom_icons(),
            icon_previews: HashMap::new(),
            icons: IconCache::default(),
            info: None,
            modal: None,
            regions: Vec::new(),
            mouse: (-1, -1),
            mouse_inside: false,
            pressed: None,
            hover_action: None,
            block_input: false,
            scroll_y: 0,
            max_scroll: 0,
            view_h: 0,
            scroll_track: RECT::default(),
            thumb_drag: None,
            nav_collapsed,
            nav_tween: Tween::new(200),
            indicator_from: 0,
            indicator_tween: Tween::new(220),
            nav_press: Tween::new(140),
            page_tween: Tween::new(200),
            lang_open: false,
            lang_tween: Tween::new(105),
            lang_press: Tween::new(120),
            toggles,
            checks: HashMap::new(),
            expanders,
            update_auto_started: false,
        };
        if just_updated {
            st.show_info(
                Severity::Success,
                &format!(
                    "{} {}{}",
                    tr("已更新至版本", "Updated to version"),
                    env!("CARGO_PKG_VERSION"),
                    tr(
                        "。請重新啟動檔案總管，讓側邊欄擴充套件改用新版本。",
                        ". Restart Explorer so the sidebar extension uses the new version.",
                    )
                ),
            );
        }
        st
    }

    /// Registers a hit region and returns its hover / press state for this frame.
    pub fn interact(&mut self, rect: RECT, action: Action, cursor: Cursor) -> Hot {
        let hovered = !self.block_input && self.mouse_inside && contains(&rect, self.mouse.0, self.mouse.1);
        let pressed = hovered && self.pressed.as_ref() == Some(&action);
        self.regions.push(Region { rect, action, cursor });
        Hot { hovered, pressed }
    }

    /// Topmost region under a point (regions pushed later are on top).
    pub fn region_at(&self, x: i32, y: i32) -> Option<&Region> {
        self.regions.iter().rev().find(|r| contains(&r.rect, x, y))
    }

    pub fn root(&self) -> &cfd_core::config::RootFolderEntry {
        &self.config.root_folders[self.selected_root.min(self.config.root_folders.len() - 1)]
    }

    pub fn root_mut(&mut self) -> &mut cfd_core::config::RootFolderEntry {
        let i = self.selected_root.min(self.config.root_folders.len() - 1);
        &mut self.config.root_folders[i]
    }

    pub fn is_dirty(&self) -> bool {
        root_snapshot(&self.config) != self.applied_snapshot
    }

    pub fn toggle_value(&self, id: ToggleId) -> bool {
        match id {
            ToggleId::ShowInSidebar => self.config.show_in_sidebar,
            ToggleId::HideDesktop => self.config.hide_from_desktop,
            ToggleId::AutoUpdate => self.config.auto_check_updates,
        }
    }

    pub fn field_text(&self, f: Field) -> &str {
        match f {
            Field::RootName => &self.root().name,
            Field::RootPath => &self.root().target_path,
            Field::RootIcon => &self.root().icon,
            Field::SubName => &self.sub_name,
            Field::SubPath => &self.sub_path,
        }
    }

    pub fn field_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::RootName => &mut self.root_mut().name,
            Field::RootPath => &mut self.root_mut().target_path,
            Field::RootIcon => &mut self.root_mut().icon,
            Field::SubName => &mut self.sub_name,
            Field::SubPath => &mut self.sub_path,
        }
    }

    /// Value the clear button restores (`None` → empty).
    pub fn field_default(f: Field) -> Option<&'static str> {
        match f {
            Field::RootIcon => Some(DEFAULT_ROOT_ICON),
            _ => None,
        }
    }

    pub fn show_info(&mut self, severity: Severity, msg: &str) {
        self.info = Some(InfoState { severity, message: msg.to_string() });
        // SAFETY: timer on our own window.
        unsafe {
            let _ = KillTimer(self.hwnd, TIMER_INFO);
            if matches!(severity, Severity::Success | Severity::Info) {
                let _ = SetTimer(self.hwnd, TIMER_INFO, 7000, None);
            }
        }
    }

    pub fn ok(&mut self, msg: &str) {
        self.show_info(Severity::Success, msg);
    }

    pub fn err(&mut self, msg: &str) {
        self.show_info(Severity::Error, msg);
    }

    pub fn start_anim(&self) {
        // SAFETY: timer on our own window.
        unsafe {
            let _ = SetTimer(self.hwnd, TIMER_ANIM, 16, None);
        }
    }

    pub fn unfocus(&mut self) {
        if self.focused.take().is_some() {
            // SAFETY: timer on our own window.
            unsafe {
                let _ = KillTimer(self.hwnd, TIMER_CARET);
            }
        }
    }

    pub fn switch_page(&mut self, p: Page) {
        if p == self.page {
            return;
        }
        self.indicator_from = self.page.index();
        self.indicator_tween.start();
        self.page = p;
        self.scroll_y = 0;
        self.page_tween.start();
        self.unfocus();
        self.start_anim();
    }

    pub fn set_expander(&mut self, id: ExpanderId, expanded: bool) {
        if let Some(e) = self.expanders.get_mut(&id) {
            if e.expanded != expanded {
                e.expanded = expanded;
                e.tween.start();
            }
        }
        self.start_anim();
    }

    pub fn ask(&mut self, c: Confirm) {
        let (title, body, ok, danger) = match &c {
            Confirm::DeleteRoot(i) => (
                tr("刪除根資料夾？", "Delete root folder?").to_string(),
                format!(
                    "{}「{}」{}",
                    tr("將從檔案總管側邊欄移除", "This removes"),
                    self.config.root_folders.get(*i).map(|r| r.name.as_str()).unwrap_or(""),
                    tr(
                        "，並刪除它底下的所有子資料夾設定。實際的資料夾與檔案不受影響。",
                        " from the Explorer sidebar and deletes all of its subfolder entries. Your actual folders and files are not touched.",
                    )
                ),
                tr("刪除", "Delete").to_string(),
                true,
            ),
            Confirm::DeleteSub(i) => (
                tr("移除子資料夾？", "Remove subfolder?").to_string(),
                format!(
                    "{}「{}」{}",
                    tr("將從側邊欄移除", "Removes"),
                    self.config.subfolders.get(*i).map(|s| s.name.as_str()).unwrap_or(""),
                    tr("。實際的資料夾不受影響。", " from the sidebar. The actual folder is not touched."),
                ),
                tr("移除", "Remove").to_string(),
                true,
            ),
            Confirm::DeleteIcon(p) => (
                tr("刪除自訂圖示？", "Delete custom icon?").to_string(),
                format!(
                    "{} {}",
                    p.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                    tr("將被刪除；使用中的根資料夾會改回預設圖示。", "will be deleted; root folders using it revert to the default icon."),
                ),
                tr("刪除", "Delete").to_string(),
                true,
            ),
            Confirm::Unregister => (
                tr("從檔案總管移除？", "Remove from Explorer?").to_string(),
                tr(
                    "所有自訂根資料夾會從側邊欄消失，設定仍會保留，之後可以再按「套用」重新加入。",
                    "All custom root folders disappear from the sidebar. Your settings are kept, so you can click Apply later to add them back.",
                )
                .to_string(),
                tr("移除", "Remove").to_string(),
                true,
            ),
            Confirm::RestartExplorer => (
                tr("重新啟動檔案總管？", "Restart Explorer?").to_string(),
                tr(
                    "工作列與所有檔案總管視窗會短暫消失後重新出現，開著的資料夾視窗會被關閉。",
                    "The taskbar and all File Explorer windows will briefly disappear and come back. Open folder windows will be closed.",
                )
                .to_string(),
                tr("重新啟動", "Restart").to_string(),
                false,
            ),
            Confirm::Cleanup => (
                tr("完全清理？", "Clean up everything?").to_string(),
                tr(
                    "將移除所有登錄項目、設定檔與自訂圖示，控制中心會回到初始狀態。此動作無法復原。",
                    "Removes every registry entry, the configuration file and all custom icons; the control center returns to its initial state. This cannot be undone.",
                )
                .to_string(),
                tr("全部清理", "Clean up").to_string(),
                true,
            ),
            Confirm::InstallUpdate => {
                let v = match updater::phase() {
                    updater::Phase::Available(m) => m.version,
                    _ => String::new(),
                };
                (
                    format!("{} {v}", tr("安裝版本", "Install version")),
                    tr(
                        "將下載安裝檔並驗證數位簽章，接著關閉控制中心並就地更新。所有設定都會保留；更新完成後控制中心會自動重新開啟。",
                        "The installer is downloaded and its digital signature verified; the control center then closes and updates in place. All settings are kept and the control center reopens when done.",
                    )
                    .to_string(),
                    tr("下載並安裝", "Download and install").to_string(),
                    false,
                )
            }
        };
        self.unfocus();
        self.lang_open = false;
        self.modal = Some(ModalState { confirm: c, title, body, ok_label: ok, danger });
    }

    pub fn register_now(&mut self) -> Result<(), String> {
        let dll = find_shell_dll().ok_or_else(|| {
            tr(
                "設定已儲存，但找不到 cfd_shell.dll。請重新安裝 CustomerFD。",
                "Settings saved, but cfd_shell.dll was not found. Please reinstall CustomerFD.",
            )
            .to_string()
        })?;
        register_namespace(&self.config, &dll)?;
        self.registered = true;
        self.applied_snapshot = root_snapshot(&self.config);
        Ok(())
    }

    fn save(&mut self) -> bool {
        match self.config.save() {
            Ok(()) => true,
            Err(e) => {
                self.err(&e);
                false
            }
        }
    }

    /// Root-folder index used by subfolder entries of the selected root.
    pub fn sub_indices(&self) -> Vec<usize> {
        let rid = &self.root().id;
        self.config.subfolders.iter().enumerate().filter(|(_, s)| &s.root_id == rid).map(|(i, _)| i).collect()
    }
}

/// Executes an action. Dialog-opening actions run without holding the state borrow, so the
/// window keeps painting while the dialog's modal loop runs.
pub unsafe fn handle_action(hwnd: HWND, action: Action) {
    match action {
        Action::BrowseIcon => {
            let filter = "Icon / Executable (*.ico;*.dll;*.exe)\0*.ico;*.dll;*.exe\0All Files (*.*)\0*.*\0\0";
            if let Some(p) = sys::prompt_open_file(hwnd, tr("選擇圖示檔案 (.ico / .dll / .exe)", "Select Icon File"), filter) {
                with_state(|st| st.root_mut().icon = p);
            }
        }
        Action::BrowseRootPath => {
            if let Some(p) = sys::browse_folder_dialog(hwnd, sys::folder_dialog_title()) {
                with_state(|st| st.root_mut().target_path = p);
            }
        }
        Action::BrowseSubPath => {
            if let Some(p) = sys::browse_folder_dialog(hwnd, sys::folder_dialog_title()) {
                with_state(|st| {
                    if st.sub_name.trim().is_empty() {
                        if let Some(n) = Path::new(&p).file_name().and_then(|n| n.to_str()) {
                            st.sub_name = n.to_string();
                        }
                    }
                    st.sub_path = p;
                });
            }
        }
        Action::ImportIcon => {
            let filter = "Images (*.png;*.jpg;*.jpeg;*.bmp;*.webp;*.gif;*.ico)\0*.png;*.jpg;*.jpeg;*.bmp;*.webp;*.gif;*.ico\0All Files (*.*)\0*.*\0\0";
            if let Some(p) = sys::prompt_open_file(hwnd, tr("選擇圖片轉換為圖示", "Select an image to convert to an icon"), filter) {
                with_state(|st| match import_image_as_ico(Path::new(&p)) {
                    Ok(ico) => {
                        st.root_mut().icon = ico.display().to_string();
                        st.custom_icons = list_custom_icons();
                        st.ok(tr("已轉換並儲存自訂圖示。", "Custom icon converted and saved."));
                    }
                    Err(e) => st.err(&e),
                });
            }
        }
        Action::ModalConfirm => {
            let confirm = with_state(|st| st.modal.take().map(|m| m.confirm)).flatten();
            if let Some(c) = confirm {
                if c == Confirm::RestartExplorer {
                    // Runs outside the borrow: it waits for the shell to come back.
                    let r = restart_explorer();
                    with_state(|st| match r {
                        Ok(()) => st.ok(tr("已重新啟動 Windows 檔案總管。", "Windows Explorer restarted.")),
                        Err(e) => st.err(&e),
                    });
                } else {
                    with_state(|st| confirmed(st, c));
                }
            }
        }
        other => {
            with_state(|st| simple_action(st, other));
        }
    }
    let _ = InvalidateRect(hwnd, None, false);
}

fn confirmed(st: &mut AppState, c: Confirm) {
    match c {
        Confirm::DeleteRoot(idx) => {
            if st.config.root_folders.len() > 1 && idx < st.config.root_folders.len() {
                let removed = st.config.root_folders.remove(idx);
                st.config.subfolders.retain(|s| s.root_id != removed.id);
                st.selected_root = st.selected_root.min(st.config.root_folders.len() - 1);
                if st.save() {
                    if st.registered {
                        if let Err(e) = st.register_now() {
                            st.err(&e);
                            return;
                        }
                    }
                    st.ok(tr("已刪除根資料夾。", "Root folder deleted."));
                }
            }
        }
        Confirm::DeleteSub(idx) => {
            if idx < st.config.subfolders.len() {
                let id = st.config.subfolders.remove(idx).id;
                if st.editing_sub.as_deref() == Some(id.as_str()) {
                    st.editing_sub = None;
                    st.sub_name.clear();
                    st.sub_path.clear();
                }
                if st.save() {
                    notify_explorer_refresh();
                    st.ok(tr("已移除子資料夾。", "Subfolder removed."));
                }
            }
        }
        Confirm::DeleteIcon(path) => {
            let path_str = path.display().to_string();
            match delete_custom_icon(&path) {
                Ok(()) => {
                    for r in &mut st.config.root_folders {
                        if r.icon.eq_ignore_ascii_case(&path_str) {
                            r.icon = DEFAULT_ROOT_ICON.to_string();
                        }
                    }
                    st.icon_previews.retain(|(p, _), _| p != &path);
                    st.icons.clear();
                    st.custom_icons = list_custom_icons();
                    st.ok(tr("已刪除自訂圖示。", "Custom icon deleted."));
                }
                Err(e) => st.err(&e),
            }
        }
        Confirm::Unregister => {
            unregister_namespace();
            st.registered = false;
            st.applied_snapshot.clear();
            st.ok(tr("已從檔案總管移除自訂資料夾。", "Removed from Explorer."));
        }
        Confirm::RestartExplorer => {}
        Confirm::Cleanup => match cleanup_all() {
            Ok(()) => {
                st.registered = false;
                st.applied_snapshot.clear();
                st.custom_icons.clear();
                st.icon_previews.clear();
                st.icons.clear();
                let lang = st.config.ui_lang;
                st.config = AppConfig { ui_lang: lang, ..AppConfig::default() };
                st.selected_root = 0;
                st.ok(tr(
                    "已清除所有登錄項目、設定檔與自訂圖示。",
                    "All registry entries, settings and custom icons were removed.",
                ));
            }
            Err(e) => st.err(&e),
        },
        Confirm::InstallUpdate => {
            if let updater::Phase::Available(m) = updater::phase() {
                updater::start_install(st.hwnd.0, m);
            }
        }
    }
}

fn simple_action(st: &mut AppState, action: Action) {
    if !matches!(action, Action::FocusField(_) | Action::ClearField(_) | Action::Block | Action::ScrollThumb) {
        st.unfocus();
    }
    if !matches!(action, Action::LangButton | Action::SetLang(_)) {
        st.lang_open = false;
    }
    match action {
        Action::Block | Action::ScrollThumb | Action::FocusField(_) => {}
        Action::NavToggle => {
            st.nav_collapsed = !st.nav_collapsed;
            st.nav_tween.start();
            st.config.nav_collapsed = st.nav_collapsed;
            let _ = st.config.save();
            st.start_anim();
        }
        Action::SwitchPage(p) => st.switch_page(p),
        Action::LangButton => {
            st.lang_open = !st.lang_open;
            st.lang_tween.start();
            st.start_anim();
        }
        Action::SetLang(l) => {
            set_lang(l);
            st.config.ui_lang = l;
            let _ = st.config.save();
            st.lang_open = false;
        }
        Action::ClearField(f) => {
            let v = AppState::field_default(f).unwrap_or("").to_string();
            *st.field_mut(f) = v;
            let len = st.field_text(f).chars().count();
            let e = st.edits.entry(f).or_default();
            e.caret = len;
            e.anchor = None;
            e.dragging = false;
        }
        Action::SelectRoot(i) => {
            if i < st.config.root_folders.len() {
                st.selected_root = i;
                st.editing_sub = None;
                st.sub_name.clear();
                st.sub_path.clear();
            }
        }
        Action::AddRoot => match st.config.next_root_folder() {
            Some(r) => {
                st.config.root_folders.push(r);
                st.selected_root = st.config.root_folders.len() - 1;
                if st.save() {
                    st.show_info(
                        Severity::Info,
                        tr(
                            "已新增根資料夾。設定名稱與圖示後，按「套用」加入檔案總管側邊欄。",
                            "Root folder added. Set its name and icon, then click Apply to add it to the Explorer sidebar.",
                        ),
                    );
                }
            }
            None => st.err(tr("根資料夾已達上限（64 個）。", "The maximum of 64 root folders is reached.")),
        },
        Action::Ask(c) => st.ask(c),
        Action::Toggle(id) => {
            match id {
                ToggleId::ShowInSidebar => st.config.show_in_sidebar = !st.config.show_in_sidebar,
                ToggleId::HideDesktop => st.config.hide_from_desktop = !st.config.hide_from_desktop,
                ToggleId::AutoUpdate => {
                    st.config.auto_check_updates = !st.config.auto_check_updates;
                    let _ = st.config.save();
                }
            }
            st.start_anim();
        }
        Action::Expander(id) => {
            let cur = st.expanders.get(&id).map(|e| e.expanded).unwrap_or(false);
            st.set_expander(id, !cur);
        }
        Action::PreviewToggle => st.preview_expanded = !st.preview_expanded,
        Action::Apply => {
            for r in &mut st.config.root_folders {
                r.name = r.name.trim().to_string();
                if r.name.is_empty() {
                    r.name = tr("自訂資料夾", "Custom Folder").to_string();
                }
                r.target_path = r.target_path.trim().to_string();
                r.icon = r.icon.trim().to_string();
                if r.icon.is_empty() {
                    r.icon = DEFAULT_ROOT_ICON.to_string();
                }
            }
            if st.save() {
                match st.register_now() {
                    Ok(()) => st.ok(tr(
                        "已儲存並更新檔案總管側邊欄。若沒有立即出現，請重新啟動檔案總管。",
                        "Saved and the Explorer sidebar was refreshed. If it does not show up, restart Explorer.",
                    )),
                    Err(e) => st.err(&e),
                }
            }
            st.icons.clear();
        }
        Action::SelectIcon(v) => st.root_mut().icon = v,
        Action::SaveSub => {
            let name = st.sub_name.trim().to_string();
            let path = st.sub_path.trim().to_string();
            if name.is_empty() || path.is_empty() {
                st.err(tr("請輸入子資料夾名稱與路徑。", "Please enter both a name and a path."));
                return;
            }
            let root_id = st.root().id.clone();
            let editing = st.editing_sub.take();
            if let Some(id) = editing {
                if let Some(item) = st.config.subfolders.iter_mut().find(|e| e.id == id) {
                    item.name = name;
                    item.target_path = path;
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
            st.sub_name.clear();
            st.sub_path.clear();
            if st.save() {
                notify_explorer_refresh();
                st.ok(tr("子資料夾清單已更新。", "Subfolder list updated."));
            }
        }
        Action::CancelSub => {
            st.editing_sub = None;
            st.sub_name.clear();
            st.sub_path.clear();
        }
        Action::SubEnabled(id) => {
            if let Some(item) = st.config.subfolders.iter_mut().find(|s| s.id == id) {
                item.enabled = !item.enabled;
                if item.enabled {
                    let mut t = Tween::new(180);
                    t.start();
                    st.checks.insert(id, t);
                    st.start_anim();
                }
                if st.save() {
                    notify_explorer_refresh();
                }
            }
        }
        Action::SubUp(idx) | Action::SubDown(idx) => {
            let up = matches!(action, Action::SubUp(_));
            let list = st.sub_indices();
            if let Some(pos) = list.iter().position(|&i| i == idx) {
                let other = if up { pos.checked_sub(1) } else { (pos + 1 < list.len()).then_some(pos + 1) };
                if let Some(o) = other {
                    st.config.subfolders.swap(idx, list[o]);
                    if st.save() {
                        notify_explorer_refresh();
                    }
                }
            }
        }
        Action::EditSub(idx) => {
            if let Some(item) = st.config.subfolders.get(idx).cloned() {
                st.editing_sub = Some(item.id);
                st.sub_name = item.name;
                st.sub_path = item.target_path;
                st.set_expander(ExpanderId::SubEditor, true);
                st.scroll_y = 0;
            }
        }
        Action::OpenSub(idx) => {
            if let Some(item) = st.config.subfolders.get(idx) {
                // SAFETY: shell open of a user-configured path.
                unsafe { sys::shell_open(&item.target_path) };
            }
        }
        Action::OpenConfigDir => {
            let dir = config_dir();
            let _ = std::fs::create_dir_all(&dir);
            // SAFETY: shell open of our own folder.
            unsafe { sys::shell_open(&dir.display().to_string()) };
        }
        Action::OpenInstallDir => {
            if let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)) {
                // SAFETY: shell open of our own folder.
                unsafe { sys::shell_open(&dir.display().to_string()) };
            }
        }
        Action::CheckUpdate => updater::start_check(st.hwnd.0),
        Action::ReleaseNotes => {
            let v = match updater::phase() {
                updater::Phase::Available(m) => Some(m.version),
                _ => None,
            };
            updater::open_release_page(v.as_deref());
        }
        Action::OpenGitHub => {
            // SAFETY: shell open of a constant URL.
            unsafe { sys::shell_open(crate::REPO_URL) };
        }
        Action::OpenLicense => {
            let local = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("LICENSE.txt")));
            match local.filter(|p| p.exists()) {
                // SAFETY: shell open of our own file / constant URL.
                Some(p) => unsafe { sys::shell_open(&p.display().to_string()) },
                None => unsafe { sys::shell_open(&format!("{}/blob/main/LICENSE", crate::REPO_URL)) },
            }
        }
        Action::DismissInfo => st.info = None,
        Action::ModalCancel => st.modal = None,
        Action::BrowseIcon
        | Action::BrowseRootPath
        | Action::BrowseSubPath
        | Action::ImportIcon
        | Action::ModalConfirm => {}
    }
}

/// Persists the update-check timestamp once a check completes.
pub fn note_update_check(st: &mut AppState) {
    if updater::take_check_completed() {
        st.config.last_update_check = now_unix();
        let _ = st.config.save();
    }
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Starts the once-a-day background check (notify only) when enabled and due.
pub fn maybe_auto_check(st: &mut AppState) {
    if st.update_auto_started || !st.config.auto_check_updates || !updater::is_configured() {
        return;
    }
    st.update_auto_started = true;
    if now_unix().saturating_sub(st.config.last_update_check) >= 24 * 3600 {
        updater::start_check(st.hwnd.0);
    }
}

/// Advances every animation; returns whether any is still running.
pub fn step_animations(st: &mut AppState) -> bool {
    let mut active = false;
    for t in [
        &mut st.nav_tween,
        &mut st.indicator_tween,
        &mut st.nav_press,
        &mut st.page_tween,
        &mut st.lang_tween,
        &mut st.lang_press,
    ] {
        active |= t.tick();
    }
    for (id, v) in st.toggles.iter_mut() {
        let target = match id {
            ToggleId::ShowInSidebar => st.config.show_in_sidebar,
            ToggleId::HideDesktop => st.config.hide_from_desktop,
            ToggleId::AutoUpdate => st.config.auto_check_updates,
        } as u8 as f32;
        active |= cfd_ui::anim::approach(v, target);
    }
    st.checks.retain(|_, t| t.tick());
    active |= !st.checks.is_empty();
    for e in st.expanders.values_mut() {
        active |= e.tween.tick();
        active |= e.press.tick();
    }
    active
}

