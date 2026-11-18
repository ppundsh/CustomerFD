#![allow(non_snake_case)]

mod enum_id;
mod folder;
mod pidl;

use cfd_core::config::{parse_guid, AppConfig, BASE_CLSID_U128};
use cfd_core::registry::{register_namespace, unregister_namespace};
use folder::VirtualRootFolder;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};
use windows::core::{implement, IUnknown, Interface, Result, GUID, HRESULT};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, HMODULE, MAX_PATH, S_FALSE,
    S_OK,
};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::Win32::System::SystemServices::DLL_PROCESS_ATTACH;

static MODULE_HANDLE: AtomicUsize = AtomicUsize::new(0);
static LOCK_COUNT: AtomicI32 = AtomicI32::new(0);
/// Live COM objects created by this DLL; `DllCanUnloadNow` must refuse while any exist,
/// otherwise explorer.exe could unload the code under a live object and crash.
static OBJECT_COUNT: AtomicI32 = AtomicI32::new(0);

/// Embedded in every COM object to keep [`OBJECT_COUNT`] accurate.
pub(crate) struct LiveObject;

impl LiveObject {
    pub(crate) fn new() -> Self {
        OBJECT_COUNT.fetch_add(1, Ordering::SeqCst);
        LiveObject
    }
}

impl Drop for LiveObject {
    fn drop(&mut self) {
        OBJECT_COUNT.fetch_sub(1, Ordering::SeqCst);
    }
}

#[implement(IClassFactory)]
struct ClassFactory {
    root_id: String,
    clsid: GUID,
}

impl IClassFactory_Impl for ClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut c_void,
    ) -> Result<()> {
        if punkouter.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        if riid.is_null() || ppvobject.is_null() {
            return Err(E_POINTER.into());
        }
        unsafe { *ppvobject = std::ptr::null_mut() };

        let folder: IUnknown =
            VirtualRootFolder::with_root(self.root_id.clone(), self.clsid).into();
        unsafe { folder.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, flock: windows::Win32::Foundation::BOOL) -> Result<()> {
        if flock.as_bool() {
            LOCK_COUNT.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCK_COUNT.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[no_mangle]
pub extern "system" fn DllMain(
    hinst_dll: HMODULE,
    fdw_reason: u32,
    _lpv_reserved: *mut c_void,
) -> bool {
    if fdw_reason == DLL_PROCESS_ATTACH {
        MODULE_HANDLE.store(hinst_dll.0 as usize, Ordering::SeqCst);
    }
    true
}

/// Runs `f` and converts a panic into `E_FAIL`: a panic must never unwind into explorer.exe.
fn ffi_guard(f: impl FnOnce() -> HRESULT) -> HRESULT {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).unwrap_or(E_FAIL)
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    ffi_guard(|| get_class_object(rclsid, riid, ppv))
}

unsafe fn get_class_object(rclsid: *const GUID, riid: *const GUID, ppv: *mut *mut c_void) -> HRESULT {
    if rclsid.is_null() || riid.is_null() || ppv.is_null() {
        return E_POINTER;
    }
    *ppv = std::ptr::null_mut();

    let target_guid = *rclsid;
    let cfg = AppConfig::load();
    let matched = cfg.root_folders.into_iter().find(|r| {
        parse_guid(&r.clsid)
            .map(|g| g == target_guid)
            .unwrap_or(false)
    });

    let (root_id, clsid) = if let Some(root) = matched {
        (root.id, target_guid)
    } else if target_guid == GUID::from_u128(BASE_CLSID_U128) {
        ("root_1".to_string(), target_guid)
    } else {
        return CLASS_E_CLASSNOTAVAILABLE;
    };

    let factory: IClassFactory = ClassFactory { root_id, clsid }.into();
    factory.query(riid, ppv)
}

#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    if LOCK_COUNT.load(Ordering::SeqCst) <= 0 && OBJECT_COUNT.load(Ordering::SeqCst) <= 0 {
        S_OK
    } else {
        S_FALSE
    }
}

fn current_dll_path() -> Option<PathBuf> {
    let hmod = MODULE_HANDLE.load(Ordering::SeqCst);
    if hmod == 0 {
        return None;
    }
    let mut buf = [0u16; MAX_PATH as usize];
    let len = unsafe { GetModuleFileNameW(HMODULE(hmod as *mut c_void), &mut buf) };
    if len == 0 {
        return None;
    }
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
}

#[no_mangle]
pub extern "system" fn DllRegisterServer() -> HRESULT {
    ffi_guard(|| {
        let Some(dll_path) = current_dll_path() else {
            return E_FAIL;
        };
        let cfg = AppConfig::load();
        match register_namespace(&cfg, &dll_path) {
            Ok(()) => S_OK,
            Err(_) => E_FAIL,
        }
    })
}

#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> HRESULT {
    ffi_guard(|| {
        unregister_namespace();
        S_OK
    })
}
