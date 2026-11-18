//! Window backdrop: Mica on Windows 11, legacy Acrylic on Windows 10, and an opaque fallback.
//!
//! Ported from cbxshell-rs (`doc/ui-design.zh-TW.md` §4). The rail and title bar are painted with
//! alpha 0 so DWM shows the backdrop there; if no blur backdrop is active the renderer must
//! flatten those pixels onto an opaque colour (see [`is_backdrop_active`]).

use std::sync::atomic::Ordering;
use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;

/// DWMSBT_MAINWINDOW (Mica). 3 = Acrylic (blurs whatever is behind), 4 = Mica Alt.
const FROSTED_BACKDROP_TYPE: i32 = 2;
/// Pre-22H2 Windows 11 Mica switch (undocumented but stable in build 22000).
const DWMWA_MICA_EFFECT_21H2: u32 = 1029;
/// Opacity of the page background (`win_bg`) laid over the Mica backdrop outside the cards, so
/// the content area shows the same wallpaper glow as the rail, only deeper.
pub const CONTENT_MICA_TINT_ALPHA: u32 = 0xB4;

static BACKDROP_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[inline]
pub fn is_backdrop_active() -> bool {
    BACKDROP_ACTIVE.load(Ordering::Acquire)
}

/// Real OS build number (`RtlGetVersion` is not affected by manifest version shims).
pub fn windows_build_number() -> u32 {
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform: u32,
        csd: [u16; 128],
    }
    type RtlGetVersionFn = unsafe extern "system" fn(*mut OsVersionInfoW) -> i32;
    // SAFETY: ntdll is always loaded; the function pointer type matches RtlGetVersion's ABI and
    // the out-structure is correctly sized and initialised.
    unsafe {
        let Ok(ntdll) = GetModuleHandleW(w!("ntdll.dll")) else {
            return 0;
        };
        let Some(proc) = windows::Win32::System::LibraryLoader::GetProcAddress(
            ntdll,
            windows::core::s!("RtlGetVersion"),
        ) else {
            return 0;
        };
        let f: RtlGetVersionFn = std::mem::transmute(proc);
        let mut info = OsVersionInfoW {
            size: std::mem::size_of::<OsVersionInfoW>() as u32,
            major: 0,
            minor: 0,
            build: 0,
            platform: 0,
            csd: [0; 128],
        };
        if f(&mut info) == 0 {
            info.build
        } else {
            0
        }
    }
}

/// Undocumented but long-stable `SetWindowCompositionAttribute(WCA_ACCENT_POLICY)` acrylic blur,
/// used only where the documented system backdrop API does not exist.
unsafe fn apply_legacy_acrylic(hwnd: HWND, dark: bool) -> bool {
    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32, // 0xAABBGGRR tint
        animation_id: u32,
    }
    #[repr(C)]
    struct WinCompAttrData {
        attrib: u32,
        pv_data: *mut std::ffi::c_void,
        cb_data: usize,
    }
    type SwcaFn =
        unsafe extern "system" fn(HWND, *mut WinCompAttrData) -> windows::Win32::Foundation::BOOL;
    const WCA_ACCENT_POLICY: u32 = 19;
    const ACCENT_ENABLE_ACRYLICBLURBEHIND: u32 = 4;

    let Ok(user32) = GetModuleHandleW(w!("user32.dll")) else {
        return false;
    };
    let Some(proc) = windows::Win32::System::LibraryLoader::GetProcAddress(
        user32,
        windows::core::s!("SetWindowCompositionAttribute"),
    ) else {
        return false;
    };
    let f: SwcaFn = std::mem::transmute(proc);
    // Win11-like tints: dark #202020 / light #F3F3F3 at ~80 % opacity.
    let tint = if dark { 0xCC20_2020 } else { 0xCCF3_F3F3 };
    let mut policy = AccentPolicy {
        accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
        accent_flags: 0,
        gradient_color: tint,
        animation_id: 0,
    };
    let mut data = WinCompAttrData {
        attrib: WCA_ACCENT_POLICY,
        pv_data: (&mut policy as *mut AccentPolicy).cast(),
        cb_data: std::mem::size_of::<AccentPolicy>(),
    };
    f(hwnd, &mut data).as_bool()
}

/// Applies the best available frosted backdrop and records whether one is active.
pub fn apply_frosted_backdrop(hwnd: HWND) {
    let active = if !crate::theme::transparency_effects_enabled() {
        false
    } else {
        let build = windows_build_number();
        // SAFETY: Win32/DWM calls on our own top-level window with correctly sized arguments.
        unsafe {
            let mica_22h2 = build >= 22621
                && DwmSetWindowAttribute(
                    hwnd,
                    DWMWA_SYSTEMBACKDROP_TYPE,
                    &FROSTED_BACKDROP_TYPE as *const _ as _,
                    4,
                )
                .is_ok();
            let mica_21h2 = || {
                build >= 22000
                    && DwmSetWindowAttribute(
                        hwnd,
                        windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(
                            DWMWA_MICA_EFFECT_21H2 as i32,
                        ),
                        &1i32 as *const _ as _,
                        4,
                    )
                    .is_ok()
            };
            if mica_22h2 || mica_21h2() {
                true
            } else if build >= 17134 {
                apply_legacy_acrylic(hwnd, crate::theme::is_dark())
            } else {
                false
            }
        }
    };
    BACKDROP_ACTIVE.store(active, Ordering::Release);
}
