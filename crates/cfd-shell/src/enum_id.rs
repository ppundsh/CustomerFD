use crate::pidl::create_item_pidl;
use cfd_core::config::SubFolderEntry;
use std::sync::Mutex;
use windows::core::{implement, HRESULT};
use windows::Win32::Foundation::{E_INVALIDARG, E_POINTER, S_FALSE, S_OK};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{IEnumIDList, IEnumIDList_Impl};

struct AptEnum(Option<IEnumIDList>);
unsafe impl Send for AptEnum {}
unsafe impl Sync for AptEnum {}

#[implement(IEnumIDList)]
pub struct VirtualEnumIDList {
    entries: Vec<SubFolderEntry>,
    index: Mutex<usize>,
    target_enum: Mutex<AptEnum>,
    _live: crate::LiveObject,
}

impl VirtualEnumIDList {
    pub fn new(entries: Vec<SubFolderEntry>, target_enum: Option<IEnumIDList>) -> Self {
        Self {
            entries,
            index: Mutex::new(0),
            target_enum: Mutex::new(AptEnum(target_enum)),
            _live: crate::LiveObject::new(),
        }
    }
}

impl IEnumIDList_Impl for VirtualEnumIDList_Impl {
    fn Next(
        &self,
        celt: u32,
        rgelt: *mut *mut ITEMIDLIST,
        pceltfetched: *mut u32,
    ) -> HRESULT {
        if rgelt.is_null() {
            return E_INVALIDARG;
        }
        let mut idx = self.index.lock().unwrap_or_else(|p| p.into_inner());
        let mut fetched = 0u32;

        while fetched < celt && *idx < self.entries.len() {
            let pidl = unsafe { create_item_pidl(&self.entries[*idx].id) };
            unsafe {
                *rgelt.add(fetched as usize) = pidl;
            }
            *idx += 1;
            fetched += 1;
        }

        if fetched < celt {
            if let Ok(guard) = self.target_enum.lock() {
                if let Some(ref te) = guard.0 {
                    let rem = celt - fetched;
                    let mut sub_fetched = 0u32;
                    let slice = unsafe {
                        std::slice::from_raw_parts_mut(rgelt.add(fetched as usize), rem as usize)
                    };
                    let hr = unsafe { te.Next(slice, Some(&mut sub_fetched)) };
                    if hr.is_ok() {
                        fetched += sub_fetched;
                    }
                }
            }
        }

        if !pceltfetched.is_null() {
            unsafe {
                *pceltfetched = fetched;
            }
        }

        if fetched == celt {
            S_OK
        } else {
            S_FALSE
        }
    }

    fn Skip(&self, celt: u32) -> HRESULT {
        let mut idx = self.index.lock().unwrap_or_else(|p| p.into_inner());
        let next = (*idx).saturating_add(celt as usize);
        if next > self.entries.len() {
            let extra = (next - self.entries.len()) as u32;
            *idx = self.entries.len();
            if let Ok(guard) = self.target_enum.lock() {
                if let Some(ref te) = guard.0 {
                    return unsafe { te.Skip(extra) };
                }
            }
            S_FALSE
        } else {
            *idx = next;
            S_OK
        }
    }

    fn Reset(&self) -> HRESULT {
        let mut idx = self.index.lock().unwrap_or_else(|p| p.into_inner());
        *idx = 0;
        if let Ok(guard) = self.target_enum.lock() {
            if let Some(ref te) = guard.0 {
                let _ = unsafe { te.Reset() };
            }
        }
        S_OK
    }

    fn Clone(&self, ppenum: *mut Option<IEnumIDList>) -> HRESULT {
        if ppenum.is_null() {
            return E_POINTER;
        }
        let idx = *self.index.lock().unwrap_or_else(|p| p.into_inner());
        let cloned_target = self.target_enum.lock().ok().and_then(|g| {
            g.0.as_ref().and_then(|te| {
                let mut out: Option<IEnumIDList> = None;
                if unsafe { te.Clone(&mut out) }.is_ok() {
                    out
                } else {
                    None
                }
            })
        });
        let cloned: IEnumIDList = VirtualEnumIDList {
            entries: self.entries.clone(),
            index: Mutex::new(idx),
            target_enum: Mutex::new(AptEnum(cloned_target)),
            _live: crate::LiveObject::new(),
        }
        .into();
        unsafe {
            *ppenum = Some(cloned);
        }
        S_OK
    }
}
