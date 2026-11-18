//! Windows 11 Fluent colour tokens, shared by the control center and the installer.
//!
//! Values follow `cbxshell-rs` (see `doc/ui-design.zh-TW.md` §3). Palette entries are GDI
//! `COLORREF`s (`0x00BBGGRR`); accent helpers come in both `_rgb` (`0x00RRGGBB`) and `_bgr`.

use crate::wstr::to_wide;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD,
};

static SYSTEM_DARK_MODE: AtomicBool = AtomicBool::new(true);
static ACCENT_BASE_RGB: AtomicU32 = AtomicU32::new(0x0000_78D4);
static ACCENT_LIGHT1_RGB: AtomicU32 = AtomicU32::new(0x004C_A0E0);
static ACCENT_LIGHT2_RGB: AtomicU32 = AtomicU32::new(0x0060_CDFF);
static ACCENT_DARK1_RGB: AtomicU32 = AtomicU32::new(0x0000_5A9E);
static ACCENT_DARK2_RGB: AtomicU32 = AtomicU32::new(0x0000_4578);

/// Linear interpolation of two `0x00RRGGBB` colours.
pub fn lerp_rgb(c1: u32, c2: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |c: u32, s: u32| ((c >> s) & 0xFF) as f32;
    let mix = |s: u32| {
        (ch(c1, s) + (ch(c2, s) - ch(c1, s)) * t)
            .round()
            .clamp(0.0, 255.0) as u32
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

/// Linear interpolation of two `COLORREF` (`0x00BBGGRR`) colours. Channel order does not
/// matter for interpolation, so this is the same arithmetic as [`lerp_rgb`].
#[inline]
pub fn lerp_bgr(c1: u32, c2: u32, t: f32) -> u32 {
    lerp_rgb(c1, c2, t)
}

#[inline]
pub fn rgb_to_bgr(rgb: u32) -> u32 {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    (b << 16) | (g << 8) | r
}

#[inline]
pub fn bgr_to_rgb(bgr: u32) -> u32 {
    rgb_to_bgr(bgr)
}

fn abgr_dword_to_rgb(abgr: u32) -> u32 {
    let r = abgr & 0xFF;
    let g = (abgr >> 8) & 0xFF;
    let b = (abgr >> 16) & 0xFF;
    (r << 16) | (g << 8) | b
}

fn reg_dword(subkey: &str, name: &str) -> Option<u32> {
    let k = to_wide(subkey);
    let n = to_wide(name);
    let mut val: u32 = 0;
    let mut size: u32 = 4;
    // SAFETY: valid NUL-terminated names and a correctly sized DWORD buffer.
    let st = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            PCWSTR(n.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut val as *mut u32).cast()),
            Some(&mut size),
        )
    };
    (st == ERROR_SUCCESS).then_some(val)
}

/// `(base, light1, light2, dark1, dark2)` of the personalisation accent palette (RGB).
fn read_accent_palette() -> (u32, u32, u32, u32, u32) {
    let k = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent");
    let n = to_wide("AccentPalette");
    let mut buf = [0u8; 32];
    let mut size = buf.len() as u32;
    // SAFETY: valid names and a 32-byte buffer whose size is passed in.
    let st = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            PCWSTR(n.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if st == ERROR_SUCCESS && size >= 24 {
        let at = |i: usize| {
            ((buf[i * 4] as u32) << 16) | ((buf[i * 4 + 1] as u32) << 8) | buf[i * 4 + 2] as u32
        };
        return (at(3), at(2), at(1), at(4), at(5));
    }
    if let Some(v) = reg_dword("Software\\Microsoft\\Windows\\DWM", "AccentColor") {
        let base = abgr_dword_to_rgb(v);
        return (
            base,
            lerp_rgb(base, 0xFFFFFF, 0.28),
            lerp_rgb(base, 0xFFFFFF, 0.45),
            lerp_rgb(base, 0x000000, 0.18),
            lerp_rgb(base, 0x000000, 0.34),
        );
    }
    (0x0078D4, 0x4CA0E0, 0x60CDFF, 0x005A9E, 0x004578)
}

/// Re-reads light/dark mode and the accent palette; applies the dark title bar to `hwnd`.
pub fn refresh_system_theme(hwnd: Option<HWND>) {
    let dark = reg_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "AppsUseLightTheme",
    )
    .map(|v| v == 0)
    .unwrap_or(true);
    SYSTEM_DARK_MODE.store(dark, Ordering::Relaxed);
    let (base, l1, l2, d1, d2) = read_accent_palette();
    ACCENT_BASE_RGB.store(base, Ordering::Relaxed);
    ACCENT_LIGHT1_RGB.store(l1, Ordering::Relaxed);
    ACCENT_LIGHT2_RGB.store(l2, Ordering::Relaxed);
    ACCENT_DARK1_RGB.store(d1, Ordering::Relaxed);
    ACCENT_DARK2_RGB.store(d2, Ordering::Relaxed);
    if let Some(h) = hwnd {
        let v: i32 = dark as i32;
        // SAFETY: DWM attribute on our own window with a correctly sized i32.
        unsafe {
            let _ = DwmSetWindowAttribute(h, DWMWA_USE_IMMERSIVE_DARK_MODE, &v as *const _ as _, 4);
        }
    }
}

/// `false` when Settings › Personalization › Colors › Transparency effects is off.
pub fn transparency_effects_enabled() -> bool {
    reg_dword(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "EnableTransparency",
    )
    .map(|v| v != 0)
    .unwrap_or(true)
}

#[inline]
pub fn is_dark() -> bool {
    SYSTEM_DARK_MODE.load(Ordering::Relaxed)
}

#[inline]
pub fn accent_rgb() -> u32 {
    ACCENT_BASE_RGB.load(Ordering::Relaxed)
}
#[inline]
pub fn accent_bgr() -> u32 {
    rgb_to_bgr(accent_rgb())
}
/// Primary button hover: base mixed with 14 % white.
#[inline]
pub fn accent_hover_bgr() -> u32 {
    rgb_to_bgr(lerp_rgb(accent_rgb(), 0xFFFFFF, 0.14))
}
/// Primary button pressed: Dark1.
#[inline]
pub fn accent_pressed_bgr() -> u32 {
    rgb_to_bgr(ACCENT_DARK1_RGB.load(Ordering::Relaxed))
}
/// Checked checkbox hover: base mixed with 16 % black.
#[inline]
pub fn accent_checkbox_hover_rgb() -> u32 {
    lerp_rgb(accent_rgb(), 0x000000, 0.16)
}
/// Text on top of an accent fill (YIQ ≥ 150 → near-black, else white), RGB.
#[inline]
pub fn accent_contrast_text_rgb() -> u32 {
    let c = accent_rgb();
    let yiq = (299 * ((c >> 16) & 0xFF) + 587 * ((c >> 8) & 0xFF) + 114 * (c & 0xFF)) / 1000;
    if yiq >= 150 {
        0x111111
    } else {
        0xFFFFFF
    }
}
#[inline]
pub fn accent_contrast_text_bgr() -> u32 {
    rgb_to_bgr(accent_contrast_text_rgb())
}
/// Accent-coloured text: Light1 (Light2 when Light1 is too dark) in dark mode, Dark1 in light mode.
pub fn accent_text_bgr() -> u32 {
    if !is_dark() {
        return rgb_to_bgr(ACCENT_DARK1_RGB.load(Ordering::Relaxed));
    }
    let l1 = ACCENT_LIGHT1_RGB.load(Ordering::Relaxed);
    let lum = (299 * ((l1 >> 16) & 0xFF) + 587 * ((l1 >> 8) & 0xFF) + 114 * (l1 & 0xFF)) / 1000;
    rgb_to_bgr(if lum < 120 {
        ACCENT_LIGHT2_RGB.load(Ordering::Relaxed)
    } else {
        l1
    })
}
/// Subtle accent tint for selected rows / info bars.
pub fn accent_subtle_bg_bgr() -> u32 {
    if is_dark() {
        rgb_to_bgr(lerp_rgb(0x202020, accent_rgb(), 0.22))
    } else {
        rgb_to_bgr(lerp_rgb(0xFFFFFF, accent_rgb(), 0.12))
    }
}
#[allow(dead_code)]
fn accent_dark2_rgb() -> u32 {
    ACCENT_DARK2_RGB.load(Ordering::Relaxed)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub win_bg: u32,
    pub sidebar_bg: u32,
    pub card_bg: u32,
    pub card_border: u32,
    pub sub_card_bg: u32,
    pub sub_card_border: u32,
    pub divider: u32,
    pub text_primary: u32,
    pub text_bright: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub text_dim: u32,
    pub text_disabled: u32,
    pub btn_bg: u32,
    pub btn_hover_bg: u32,
    pub btn_hover_border: u32,
    pub btn_border: u32,
    pub btn_disabled_bg: u32,
    pub btn_disabled_border: u32,
    pub sel_unselected_bg: u32,
    pub sel_unselected_hover_bg: u32,
    pub sel_unselected_border: u32,
    pub sel_unselected_text: u32,
    pub status_ok: u32,
    pub status_warn: u32,
    pub status_err: u32,
    pub caution_bg: u32,
    pub caution_border: u32,
    pub caution_text: u32,
    pub success_bg: u32,
    pub success_border: u32,
    pub modal_backdrop: u32,
    pub scrollbar_track: u32,
    pub scrollbar_thumb: u32,
}

/// Current palette for the system light / dark mode.
pub fn theme() -> Palette {
    if is_dark() {
        Palette {
            dark: true,
            win_bg: 0x001C1919,
            sidebar_bg: 0x001B1B1E,
            card_bg: 0x00242020,
            card_border: 0x00332D2D,
            sub_card_bg: 0x001B1717,
            sub_card_border: 0x002F2828,
            divider: 0x002F2828,
            text_primary: 0x00F0F0F0,
            text_bright: 0x00FFFFFF,
            text_secondary: 0x00D0D0D0,
            text_muted: 0x00A0A0A0,
            text_dim: 0x00888080,
            text_disabled: 0x00888888,
            btn_bg: 0x00202022,
            btn_hover_bg: 0x002B2B2E,
            btn_hover_border: 0x00666666,
            btn_border: 0x003D3D44,
            btn_disabled_bg: 0x0036363B,
            btn_disabled_border: 0x004E4E56,
            sel_unselected_bg: 0x00202022,
            sel_unselected_hover_bg: 0x002B2B2E,
            sel_unselected_border: 0x003D3D44,
            sel_unselected_text: 0x00D0D0D0,
            status_ok: 0x006CCB5F,
            status_warn: 0x0033B8FF,
            status_err: 0x005555EE,
            caution_bg: 0x00182C3D,
            caution_border: 0x0030649E,
            caution_text: 0x009CE0FF,
            success_bg: 0x00183A1C,
            success_border: 0x00307A3A,
            modal_backdrop: 0x00141212,
            scrollbar_track: 0x00201C1C,
            scrollbar_thumb: 0x004A4444,
        }
    } else {
        Palette {
            dark: false,
            win_bg: 0x00F3F3F3,
            sidebar_bg: 0x00ECECEC,
            card_bg: 0x00FFFFFF,
            card_border: 0x00E5E5E5,
            sub_card_bg: 0x00F9F9F9,
            sub_card_border: 0x00E5E5E5,
            divider: 0x00E5E5E5,
            text_primary: 0x001A1A1A,
            text_bright: 0x00111111,
            text_secondary: 0x003A3A3A,
            text_muted: 0x005C5C5C,
            text_dim: 0x00757575,
            text_disabled: 0x009E9E9E,
            btn_bg: 0x00FBFBFB,
            btn_hover_bg: 0x00F2F2F2,
            btn_hover_border: 0x00BDBDBD,
            btn_border: 0x00DCDCDC,
            btn_disabled_bg: 0x00F3F3F3,
            btn_disabled_border: 0x00E0E0E0,
            sel_unselected_bg: 0x00FBFBFB,
            sel_unselected_hover_bg: 0x00F0F0F0,
            sel_unselected_border: 0x00DCDCDC,
            sel_unselected_text: 0x001A1A1A,
            status_ok: 0x000F7B0F,
            status_warn: 0x00005D9D,
            status_err: 0x001C2BC4,
            caution_bg: 0x00CEF4FF,
            caution_border: 0x0000B8E6,
            caution_text: 0x00004466,
            success_bg: 0x00DFF6DF,
            success_border: 0x0090CC90,
            modal_backdrop: 0x00E6E6E6,
            scrollbar_track: 0x00EBEBEB,
            scrollbar_thumb: 0x00B8B8B8,
        }
    }
}
