use std::ptr;
use windows::Win32::System::Com::CoTaskMemAlloc;
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, SHITEMID};

const MAGIC: [u8; 4] = *b"CFDS";

#[inline]
pub unsafe fn il_is_empty(pidl: *const ITEMIDLIST) -> bool {
    pidl.is_null() || (*pidl).mkid.cb == 0
}

#[inline]
pub unsafe fn il_next(pidl: *const ITEMIDLIST) -> *const ITEMIDLIST {
    if il_is_empty(pidl) {
        return ptr::null();
    }
    let cb = (*pidl).mkid.cb as usize;
    (pidl as *const u8).add(cb) as *const ITEMIDLIST
}

pub unsafe fn il_size(pidl: *const ITEMIDLIST) -> usize {
    if pidl.is_null() {
        return 0;
    }
    let mut total = 0usize;
    let mut cur = pidl;
    while !cur.is_null() && (*cur).mkid.cb != 0 {
        let cb = (*cur).mkid.cb as usize;
        total += cb;
        cur = (cur as *const u8).add(cb) as *const ITEMIDLIST;
    }
    total + 2 // 2-byte null terminator
}

pub unsafe fn il_clone(pidl: *const ITEMIDLIST) -> *mut ITEMIDLIST {
    if pidl.is_null() {
        return ptr::null_mut();
    }
    let size = il_size(pidl);
    let mem = CoTaskMemAlloc(size) as *mut u8;
    if mem.is_null() {
        return ptr::null_mut();
    }
    ptr::copy_nonoverlapping(pidl as *const u8, mem, size);
    mem as *mut ITEMIDLIST
}

/// Creates a single-item PIDL containing `MAGIC` + `id` UTF-8 bytes + 2-byte terminator.
pub unsafe fn create_item_pidl(id: &str) -> *mut ITEMIDLIST {
    let id_bytes = id.as_bytes();
    let id_len = id_bytes.len().min(u16::MAX as usize - 16);
    // SHITEMID.cb (2) + MAGIC (4) + id_len (2) + id_bytes
    let item_cb = 2 + 4 + 2 + id_len;
    let total = item_cb + 2; // + null terminator

    let mem = CoTaskMemAlloc(total) as *mut u8;
    if mem.is_null() {
        return ptr::null_mut();
    }
    ptr::write_bytes(mem, 0, total);

    // Write cb (u16)
    let cb_bytes = (item_cb as u16).to_le_bytes();
    ptr::copy_nonoverlapping(cb_bytes.as_ptr(), mem, 2);

    // Write MAGIC
    ptr::copy_nonoverlapping(MAGIC.as_ptr(), mem.add(2), 4);

    // Write id_len (u16)
    let len_bytes = (id_len as u16).to_le_bytes();
    ptr::copy_nonoverlapping(len_bytes.as_ptr(), mem.add(6), 2);

    // Write id_bytes
    ptr::copy_nonoverlapping(id_bytes.as_ptr(), mem.add(8), id_len);

    mem as *mut ITEMIDLIST
}

/// Extracts the `id` string from the first SHITEMID in `pidl`.
pub unsafe fn parse_item_id(pidl: *const ITEMIDLIST) -> Option<String> {
    if il_is_empty(pidl) {
        return None;
    }
    let cb = (*pidl).mkid.cb as usize;
    if cb < 8 {
        return None;
    }
    let base = pidl as *const u8;
    let magic = std::slice::from_raw_parts(base.add(2), 4);
    if magic != MAGIC {
        return None;
    }
    let len_bytes = [*(base.add(6)), *(base.add(7))];
    let id_len = u16::from_le_bytes(len_bytes) as usize;
    if 8 + id_len > cb {
        return None;
    }
    let id_slice = std::slice::from_raw_parts(base.add(8), id_len);
    std::str::from_utf8(id_slice).ok().map(|s| s.to_string())
}

pub unsafe fn alloc_pwstr(s: &str) -> windows::core::PWSTR {
    let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    let byte_len = wide.len() * 2;
    let mem = CoTaskMemAlloc(byte_len) as *mut u16;
    if !mem.is_null() {
        ptr::copy_nonoverlapping(wide.as_ptr(), mem, wide.len());
    }
    windows::core::PWSTR(mem)
}

#[allow(dead_code)]
fn _assert_shitemid_size() {
    let _ = std::mem::size_of::<SHITEMID>();
}
