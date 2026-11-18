use crate::enum_id::VirtualEnumIDList;
use crate::pidl::{alloc_pwstr, il_clone, il_is_empty, il_next, parse_item_id};
use cfd_core::config::{to_wide, AppConfig, SubFolderEntry, BASE_CLSID_U128};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::path::Path;
use std::sync::Mutex;
use windows::core::{
    implement, ComObject, Error, IUnknownImpl, Interface, Result, GUID, HRESULT, PCWSTR,
};
use windows::Win32::Foundation::{
    E_FAIL, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL, E_POINTER, HWND, S_OK,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoTaskMemFree, IBindCtx, IPersist, IPersist_Impl, CLSCTX_INPROC_SERVER,
};
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET, STRRET_0, STRRET_WSTR};
use windows::Win32::UI::Shell::{
    IDefaultExtractIconInit, IEnumIDList, IExtractIconW, ILCombine, ILFree, IPersistFolder,
    IPersistFolder2, IPersistFolder2_Impl, IPersistFolder3, IPersistFolder_Impl, IShellFolder,
    IShellFolder_Impl, IShellView, SHCreateDefaultExtractIcon, SHCreateShellFolderView,
    SHGetDesktopFolder, SHGetNameFromIDList, PERSIST_FOLDER_TARGET_INFO, SFV_CREATE,
    SHCONTF_FOLDERS, SHGDN_FORPARSING, SHGDN_INFOLDER, SIGDN_DESKTOPABSOLUTEPARSING,
};

const SFGAO_CANCOPY: u32 = 0x0000_0001;
const SFGAO_CANLINK: u32 = 0x0000_0004;
const SFGAO_BROWSABLE: u32 = 0x0800_0000;
const SFGAO_FILESYSANCESTOR: u32 = 0x1000_0000;
const SFGAO_FOLDER: u32 = 0x2000_0000;
const SFGAO_HASSUBFOLDER: u32 = 0x8000_0000;

/// The system file-system folder (`CLSID_ShellFSFolder`); with `IPersistFolder3::InitializeEx`
/// it shows a real folder's contents while reporting a virtual location of our choosing.
const CLSID_SHELL_FS_FOLDER: GUID = GUID::from_u128(0xF3364BA0_65B9_11CE_A9BA_00AA004AE837);
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;

fn is_unc_or_dir(target: &str) -> bool {
    let t = target.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with("\\\\") {
        return true;
    }
    Path::new(t).is_dir()
}

#[implement(IShellFolder, IPersistFolder2, IPersistFolder, IPersist)]
pub struct VirtualRootFolder {
    root_id: String,
    clsid: GUID,
    pidl_root: Mutex<Option<usize>>,
    _live: crate::LiveObject,
}

impl VirtualRootFolder {
    pub fn with_root(root_id: String, clsid: GUID) -> Self {
        Self {
            root_id,
            clsid,
            pidl_root: Mutex::new(None),
            _live: crate::LiveObject::new(),
        }
    }
}

impl Default for VirtualRootFolder {
    fn default() -> Self {
        Self {
            root_id: "root_1".to_string(),
            clsid: GUID::from_u128(BASE_CLSID_U128),
            pidl_root: Mutex::new(None),
            _live: crate::LiveObject::new(),
        }
    }
}

impl Drop for VirtualRootFolder {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.pidl_root.lock() {
            if let Some(ptr) = guard.take() {
                unsafe {
                    CoTaskMemFree(Some(ptr as *const c_void));
                }
            }
        }
    }
}

impl VirtualRootFolder {
    fn find_entry(&self, id: &str) -> Option<SubFolderEntry> {
        let cfg = AppConfig::load();
        cfg.subfolders
            .into_iter()
            .find(|e| e.enabled && e.root_id == self.root_id && e.id == id)
    }

    fn find_root_target_path(&self) -> Option<String> {
        let cfg = AppConfig::load();
        cfg.root_folders
            .into_iter()
            .find(|r| r.id == self.root_id)
            .map(|r| r.target_path.trim().to_string())
            .filter(|p| !p.is_empty())
    }

    unsafe fn bind_path_folder(target: &str) -> Result<IShellFolder> {
        let t = target.trim();
        if t.is_empty() {
            return Err(E_FAIL.into());
        }
        let desktop = SHGetDesktopFolder()?;
        let path_w = to_wide(t);
        let mut target_pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        let mut attrs = 0u32;
        desktop.ParseDisplayName(
            HWND::default(),
            None,
            PCWSTR(path_w.as_ptr()),
            None,
            &mut target_pidl,
            &mut attrs,
        )?;
        if target_pidl.is_null() {
            return Err(E_FAIL.into());
        }
        let res: Result<IShellFolder> = desktop.BindToObject(target_pidl, None);
        CoTaskMemFree(Some(target_pidl as *const c_void));
        res
    }

    /// Desktop-absolute parsing name of this root (e.g. `::{9F3B4A10-…}`).
    fn root_parsing_name(&self) -> String {
        if let Some(root) = self.root_abs_pidl() {
            let name = unsafe {
                SHGetNameFromIDList(root as *const ITEMIDLIST, SIGDN_DESKTOPABSOLUTEPARSING)
            };
            unsafe { ILFree(Some(root as *const ITEMIDLIST)) };
            if let Ok(p) = name {
                let s = unsafe { p.to_string() }.unwrap_or_default();
                unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
                if !s.is_empty() {
                    return s;
                }
            }
        }
        format!("::{}", cfd_core::config::format_guid(&self.clsid))
    }

    /// Clone of this folder's absolute (virtual) PIDL, if Explorer initialised us.
    fn root_abs_pidl(&self) -> Option<*mut ITEMIDLIST> {
        let guard = self.pidl_root.lock().unwrap_or_else(|p| p.into_inner());
        match *guard {
            Some(ptr) if ptr != 0 => Some(unsafe { il_clone(ptr as *const ITEMIDLIST) }),
            _ => None,
        }
    }

    /// Binds the real folder `target` as a *delegate* file-system folder whose own location is
    /// `virtual_abs` (the PIDL Explorer navigated to). Without this, Explorer receives a folder
    /// that reports the real path as its location, does not match it to the navigation, and
    /// leaves the address bar and breadcrumb on the previous folder.
    unsafe fn bind_delegate_folder(
        target: &str,
        virtual_abs: *const ITEMIDLIST,
    ) -> Result<IShellFolder> {
        let t = target.trim();
        if t.is_empty() || virtual_abs.is_null() {
            return Err(E_FAIL.into());
        }
        let desktop = SHGetDesktopFolder()?;
        let path_w = to_wide(t);
        let mut target_pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        let mut attrs = 0u32;
        desktop.ParseDisplayName(
            HWND::default(),
            None,
            PCWSTR(path_w.as_ptr()),
            None,
            &mut target_pidl,
            &mut attrs,
        )?;
        if target_pidl.is_null() {
            return Err(E_FAIL.into());
        }
        let result = (|| -> Result<IShellFolder> {
            let pf3: IPersistFolder3 =
                CoCreateInstance(&CLSID_SHELL_FS_FOLDER, None, CLSCTX_INPROC_SERVER)?;
            let mut info: PERSIST_FOLDER_TARGET_INFO = std::mem::zeroed();
            info.pidlTargetFolder = target_pidl;
            let w: Vec<u16> = t.encode_utf16().collect();
            if w.len() < info.szTargetParsingName.len() {
                info.szTargetParsingName[..w.len()].copy_from_slice(&w);
            }
            info.dwAttributes = FILE_ATTRIBUTE_DIRECTORY;
            info.csidl = -1;
            pf3.InitializeEx(None, virtual_abs, &info)?;
            pf3.cast::<IShellFolder>()
        })();
        CoTaskMemFree(Some(target_pidl as *const c_void));
        result
    }

    unsafe fn bind_target_folder(&self, id: &str) -> Result<IShellFolder> {
        let entry = self.find_entry(id).ok_or_else(|| Error::from(E_FAIL))?;
        if let Some(root) = self.root_abs_pidl() {
            let item = crate::pidl::create_item_pidl(id);
            let abs = ILCombine(
                Some(root as *const ITEMIDLIST),
                Some(item as *const ITEMIDLIST),
            );
            CoTaskMemFree(Some(item as *const c_void));
            ILFree(Some(root as *const ITEMIDLIST));
            if !abs.is_null() {
                let r = Self::bind_delegate_folder(&entry.target_path, abs);
                ILFree(Some(abs as *const ITEMIDLIST));
                crate::trace(|| {
                    format!(
                        "bind_target_folder {id}: delegate {:?}",
                        r.as_ref().map(|_| "ok").map_err(|e| e.code())
                    )
                });
                if r.is_ok() {
                    return r;
                }
            }
        }
        Self::bind_path_folder(&entry.target_path)
    }

    unsafe fn bind_root_target_folder(&self) -> Result<IShellFolder> {
        let target = self
            .find_root_target_path()
            .ok_or_else(|| Error::from(E_FAIL))?;
        if let Some(root) = self.root_abs_pidl() {
            let r = Self::bind_delegate_folder(&target, root);
            ILFree(Some(root as *const ITEMIDLIST));
            if r.is_ok() {
                return r;
            }
        }
        Self::bind_path_folder(&target)
    }
}

impl IPersist_Impl for VirtualRootFolder_Impl {
    fn GetClassID(&self) -> Result<GUID> {
        Ok(self.clsid)
    }
}

impl IPersistFolder_Impl for VirtualRootFolder_Impl {
    fn Initialize(&self, pidl: *const ITEMIDLIST) -> Result<()> {
        let mut guard = self.pidl_root.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(old) = guard.take() {
            unsafe { CoTaskMemFree(Some(old as *const c_void)) };
        }
        if !pidl.is_null() {
            let cloned = unsafe { il_clone(pidl) };
            *guard = Some(cloned as usize);
        }
        Ok(())
    }
}

impl IPersistFolder2_Impl for VirtualRootFolder_Impl {
    fn GetCurFolder(&self) -> Result<*mut ITEMIDLIST> {
        let guard = self.pidl_root.lock().unwrap_or_else(|p| p.into_inner());
        match *guard {
            Some(ptr) if ptr != 0 => Ok(unsafe { il_clone(ptr as *const ITEMIDLIST) }),
            _ => Err(E_FAIL.into()),
        }
    }
}

impl IShellFolder_Impl for VirtualRootFolder_Impl {
    fn ParseDisplayName(
        &self,
        hwnd: HWND,
        pbc: Option<&IBindCtx>,
        pszdisplayname: &PCWSTR,
        pcheaten: *const u32,
        ppidl: *mut *mut ITEMIDLIST,
        pdwattributes: *mut u32,
    ) -> Result<()> {
        if ppidl.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe { *ppidl = std::ptr::null_mut() };
        let full = unsafe { pszdisplayname.to_string() }.unwrap_or_default();
        let (head, rest) = match full.split_once('\\') {
            Some((h, r)) => (h.to_string(), r.trim_start_matches('\\').to_string()),
            None => (full.clone(), String::new()),
        };
        let cfg = AppConfig::load();
        if let Some(entry) = cfg
            .subfolders
            .iter()
            .find(|e| e.enabled && e.root_id == self.root_id && (e.id == head || e.name == head))
        {
            let item = unsafe { crate::pidl::create_item_pidl(&entry.id) };
            if rest.is_empty() {
                unsafe {
                    *ppidl = item;
                    if !pdwattributes.is_null() {
                        *pdwattributes &= SFGAO_FOLDER | SFGAO_HASSUBFOLDER | SFGAO_BROWSABLE;
                    }
                }
                crate::trace(|| format!("ParseDisplayName {full:?} -> item {}", entry.id));
                return Ok(());
            }
            // "id\\sub\\dir": parse the remainder inside the mapped folder and append it.
            let target_sf = match unsafe { self.bind_target_folder(&entry.id) } {
                Ok(sf) => sf,
                Err(e) => {
                    unsafe { CoTaskMemFree(Some(item as *const c_void)) };
                    return Err(e);
                }
            };
            let rest_w = to_wide(&rest);
            let mut child: *mut ITEMIDLIST = std::ptr::null_mut();
            let pbc_raw = pbc.map(|b| b.as_raw()).unwrap_or_default();
            let hr = unsafe {
                (Interface::vtable(&target_sf).ParseDisplayName)(
                    Interface::as_raw(&target_sf),
                    hwnd,
                    pbc_raw,
                    PCWSTR(rest_w.as_ptr()),
                    std::ptr::null_mut(),
                    &mut child,
                    pdwattributes,
                )
            };
            let result = if hr.is_ok() && !child.is_null() {
                let combined = unsafe {
                    ILCombine(
                        Some(item as *const ITEMIDLIST),
                        Some(child as *const ITEMIDLIST),
                    )
                };
                unsafe { *ppidl = combined };
                Ok(())
            } else {
                Err(Error::from(if hr.is_err() { hr } else { E_FAIL }))
            };
            unsafe {
                CoTaskMemFree(Some(item as *const c_void));
                if !child.is_null() {
                    CoTaskMemFree(Some(child as *const c_void));
                }
            }
            crate::trace(|| {
                format!(
                    "ParseDisplayName {full:?} -> {:?}",
                    result.as_ref().map(|_| "ok")
                )
            });
            return result;
        }
        if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
            let pbc_raw = pbc.map(|b| b.as_raw()).unwrap_or_default();
            let hr = unsafe {
                (Interface::vtable(&root_sf).ParseDisplayName)(
                    Interface::as_raw(&root_sf),
                    hwnd,
                    pbc_raw,
                    *pszdisplayname,
                    pcheaten as *mut u32,
                    ppidl,
                    pdwattributes,
                )
            };
            return hr.ok();
        }
        Err(E_INVALIDARG.into())
    }

    fn EnumObjects(
        &self,
        hwnd: HWND,
        grfflags: u32,
        ppenumidlist: *mut Option<IEnumIDList>,
    ) -> HRESULT {
        if ppenumidlist.is_null() {
            return E_POINTER;
        }
        let want_folders = (grfflags & (SHCONTF_FOLDERS.0 as u32)) != 0;
        let entries = if want_folders {
            AppConfig::load()
                .subfolders
                .into_iter()
                .filter(|e| e.enabled && e.root_id == self.root_id)
                .collect()
        } else {
            Vec::new()
        };

        let target_enum = if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
            let mut out: Option<IEnumIDList> = None;
            if unsafe { root_sf.EnumObjects(hwnd, grfflags, &mut out) }.is_ok() {
                out
            } else {
                None
            }
        } else {
            None
        };

        let enumerator: IEnumIDList = VirtualEnumIDList::new(entries, target_enum).into();
        unsafe {
            *ppenumidlist = Some(enumerator);
        }
        S_OK
    }

    fn BindToObject(
        &self,
        pidl: *const ITEMIDLIST,
        pbc: Option<&IBindCtx>,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        if ppv.is_null() || riid.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe { *ppv = std::ptr::null_mut() };

        if let Some(id) = unsafe { parse_item_id(pidl) } {
            let target_sf = unsafe { self.bind_target_folder(&id)? };
            let rest = unsafe { il_next(pidl) };
            if unsafe { il_is_empty(rest) } {
                unsafe { target_sf.query(riid, ppv).ok() }
            } else {
                let pbc_raw = pbc.map(|b| b.as_raw()).unwrap_or_default();
                let hr = unsafe {
                    (Interface::vtable(&target_sf).BindToObject)(
                        Interface::as_raw(&target_sf),
                        rest,
                        pbc_raw,
                        riid,
                        ppv,
                    )
                };
                hr.ok()
            }
        } else if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
            let pbc_raw = pbc.map(|b| b.as_raw()).unwrap_or_default();
            let hr = unsafe {
                (Interface::vtable(&root_sf).BindToObject)(
                    Interface::as_raw(&root_sf),
                    pidl,
                    pbc_raw,
                    riid,
                    ppv,
                )
            };
            hr.ok()
        } else {
            Err(E_INVALIDARG.into())
        }
    }

    fn BindToStorage(
        &self,
        pidl: *const ITEMIDLIST,
        pbc: Option<&IBindCtx>,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        self.BindToObject(pidl, pbc, riid, ppv)
    }

    fn CompareIDs(
        &self,
        lparam: windows::Win32::Foundation::LPARAM,
        pidl1: *const ITEMIDLIST,
        pidl2: *const ITEMIDLIST,
    ) -> HRESULT {
        let id1 = unsafe { parse_item_id(pidl1) };
        let id2 = unsafe { parse_item_id(pidl2) };
        match (id1, id2) {
            (Some(a), Some(b)) => {
                if a == b {
                    let rest1 = unsafe { il_next(pidl1) };
                    let rest2 = unsafe { il_next(pidl2) };
                    let empty1 = unsafe { il_is_empty(rest1) };
                    let empty2 = unsafe { il_is_empty(rest2) };
                    if empty1 && empty2 {
                        return S_OK;
                    } else if empty1 {
                        return HRESULT(0x0000_FFFFu32 as i32);
                    } else if empty2 {
                        return HRESULT(1);
                    } else if let Ok(target_sf) = unsafe { self.bind_target_folder(&a) } {
                        return unsafe { target_sf.CompareIDs(lparam, rest1, rest2) };
                    }
                    return S_OK;
                }
                let cfg = AppConfig::load();
                let idx1 = cfg
                    .subfolders
                    .iter()
                    .position(|e| e.id == a)
                    .unwrap_or(usize::MAX);
                let idx2 = cfg
                    .subfolders
                    .iter()
                    .position(|e| e.id == b)
                    .unwrap_or(usize::MAX);
                if idx1 < idx2 {
                    HRESULT(0x0000_FFFFu32 as i32)
                } else if idx1 > idx2 {
                    HRESULT(1)
                } else {
                    match a.cmp(&b) {
                        std::cmp::Ordering::Less => HRESULT(0x0000_FFFFu32 as i32),
                        std::cmp::Ordering::Greater => HRESULT(1),
                        std::cmp::Ordering::Equal => S_OK,
                    }
                }
            }
            (Some(_), None) => HRESULT(0x0000_FFFFu32 as i32),
            (None, Some(_)) => HRESULT(1),
            (None, None) => {
                if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
                    unsafe { root_sf.CompareIDs(lparam, pidl1, pidl2) }
                } else {
                    E_INVALIDARG
                }
            }
        }
    }

    fn CreateViewObject(
        &self,
        hwndowner: HWND,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        if ppv.is_null() || riid.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe { *ppv = std::ptr::null_mut() };

        if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
            let hr = unsafe {
                (Interface::vtable(&root_sf).CreateViewObject)(
                    Interface::as_raw(&root_sf),
                    hwndowner,
                    riid,
                    ppv,
                )
            };
            if hr.is_ok() {
                return Ok(());
            }
        }

        let iid = unsafe { *riid };
        if iid == IShellView::IID {
            let self_sf: IShellFolder = {
                let com: ComObject<VirtualRootFolder> = self.to_object();
                com.to_interface()
            };
            let sfv_create = SFV_CREATE {
                cbSize: std::mem::size_of::<SFV_CREATE>() as u32,
                pshf: ManuallyDrop::new(Some(self_sf)),
                psvOuter: ManuallyDrop::new(None),
                psfvcb: ManuallyDrop::new(None),
            };
            let view = unsafe { SHCreateShellFolderView(&sfv_create)? };
            ManuallyDrop::into_inner(sfv_create.pshf);
            return unsafe { view.query(riid, ppv).ok() };
        }

        Err(E_NOINTERFACE.into())
    }

    fn GetAttributesOf(
        &self,
        cidl: u32,
        apidl: *const *const ITEMIDLIST,
        rgfinout: *mut u32,
    ) -> Result<()> {
        if rgfinout.is_null() {
            return Err(E_POINTER.into());
        }
        if cidl == 0 || apidl.is_null() {
            unsafe {
                *rgfinout &= SFGAO_FOLDER | SFGAO_HASSUBFOLDER | SFGAO_BROWSABLE;
            }
            return Ok(());
        }

        let mut common = unsafe { *rgfinout };
        for i in 0..cidl as usize {
            let pidl = unsafe { *apidl.add(i) };
            let rest = unsafe { il_next(pidl) };
            if let Some(id) = unsafe { parse_item_id(pidl) } {
                if unsafe { il_is_empty(rest) } {
                    let mut item_attrs = SFGAO_FOLDER
                        | SFGAO_BROWSABLE
                        | SFGAO_FILESYSANCESTOR
                        | SFGAO_CANCOPY
                        | SFGAO_CANLINK;
                    if let Some(entry) = self.find_entry(&id) {
                        if is_unc_or_dir(&entry.target_path) {
                            // Not SFGAO_FILESYSTEM: the item's parsing name is virtual, not a path.
                            item_attrs |= SFGAO_HASSUBFOLDER;
                        }
                    }
                    common &= item_attrs;
                } else if let Ok(target_sf) = unsafe { self.bind_target_folder(&id) } {
                    let mut sub_attrs = common;
                    unsafe {
                        let _ = target_sf.GetAttributesOf(&[rest], &mut sub_attrs);
                    }
                    common &= sub_attrs;
                }
            } else if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
                let mut sub_attrs = common;
                unsafe {
                    let _ = root_sf.GetAttributesOf(&[pidl], &mut sub_attrs);
                }
                common &= sub_attrs;
            }
        }
        unsafe { *rgfinout = common };
        Ok(())
    }

    fn GetUIObjectOf(
        &self,
        hwndowner: HWND,
        cidl: u32,
        apidl: *const *const ITEMIDLIST,
        riid: *const GUID,
        rgfreserved: *const u32,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        if ppv.is_null() || riid.is_null() || apidl.is_null() || cidl == 0 {
            return Err(E_INVALIDARG.into());
        }
        unsafe { *ppv = std::ptr::null_mut() };

        let pidl = unsafe { *apidl };
        let rest = unsafe { il_next(pidl) };
        if let Some(id) = unsafe { parse_item_id(pidl) } {
            if !unsafe { il_is_empty(rest) } {
                let target_sf = unsafe { self.bind_target_folder(&id)? };
                let hr = unsafe {
                    (Interface::vtable(&target_sf).GetUIObjectOf)(
                        Interface::as_raw(&target_sf),
                        hwndowner,
                        1,
                        &rest,
                        riid,
                        rgfreserved,
                        ppv,
                    )
                };
                return hr.ok();
            }
        } else if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
            let hr = unsafe {
                (Interface::vtable(&root_sf).GetUIObjectOf)(
                    Interface::as_raw(&root_sf),
                    hwndowner,
                    cidl,
                    apidl,
                    riid,
                    rgfreserved,
                    ppv,
                )
            };
            return hr.ok();
        }

        let iid = unsafe { *riid };
        if iid == IExtractIconW::IID {
            let icon_init: IDefaultExtractIconInit = unsafe { SHCreateDefaultExtractIcon()? };
            let icon_module = to_wide("shell32.dll");
            unsafe {
                icon_init.SetNormalIcon(PCWSTR(icon_module.as_ptr()), -4)?;
                icon_init.SetOpenIcon(PCWSTR(icon_module.as_ptr()), -5)?;
                return icon_init.query(riid, ppv).ok();
            }
        }

        Err(E_NOINTERFACE.into())
    }

    fn GetDisplayNameOf(
        &self,
        pidl: *const ITEMIDLIST,
        uflags: windows::Win32::UI::Shell::SHGDNF,
        pname: *mut STRRET,
    ) -> Result<()> {
        if pname.is_null() {
            return Err(E_POINTER.into());
        }
        if let Some(id) = unsafe { parse_item_id(pidl) } {
            let rest = unsafe { il_next(pidl) };
            if !unsafe { il_is_empty(rest) } {
                let target_sf = unsafe { self.bind_target_folder(&id)? };
                return unsafe { target_sf.GetDisplayNameOf(rest, uflags, pname) };
            }

            let entry = self.find_entry(&id).ok_or_else(|| Error::from(E_FAIL))?;
            let for_parsing = (uflags.0 & SHGDN_FORPARSING.0) != 0;
            let in_folder = (uflags.0 & SHGDN_INFOLDER.0) != 0;
            // Parsing names must round-trip through ParseDisplayName to this same virtual PIDL:
            // Explorer re-parses them to update the address bar. Returning the real target path
            // here made it resolve to a different location and leave the address bar unchanged.
            let text = match (for_parsing, in_folder) {
                (true, true) => entry.id.clone(),
                (true, false) => format!("{}\\{}", self.root_parsing_name(), entry.id),
                _ => entry.name.clone(),
            };
            crate::trace(|| {
                format!(
                    "GetDisplayNameOf item {} flags=0x{:X} -> {text:?}",
                    entry.id, uflags.0
                )
            });

            unsafe {
                (*pname).uType = STRRET_WSTR.0 as u32;
                (*pname).Anonymous = STRRET_0 {
                    pOleStr: alloc_pwstr(&text),
                };
            }
            Ok(())
        } else if let Ok(root_sf) = unsafe { self.bind_root_target_folder() } {
            unsafe { root_sf.GetDisplayNameOf(pidl, uflags, pname) }
        } else {
            Err(E_INVALIDARG.into())
        }
    }

    fn SetNameOf(
        &self,
        _hwnd: HWND,
        _pidl: *const ITEMIDLIST,
        _pszname: &PCWSTR,
        _uflags: windows::Win32::UI::Shell::SHGDNF,
        _ppidlout: *mut *mut ITEMIDLIST,
    ) -> Result<()> {
        Err(E_NOTIMPL.into())
    }
}
