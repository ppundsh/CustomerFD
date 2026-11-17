use cfd_core::config::to_wide;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE};
use windows::Win32::System::Registry::{
    RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD,
};

static SYSTEM_DARK_MODE: AtomicBool = AtomicBool::new(true);
static ACCENT_RGB: AtomicU32 = AtomicU32::new(0x00C86A38);
static ACCENT_LIGHT1_RGB: AtomicU32 = AtomicU32::new(0x00DE824F);
static ACCENT_DARK1_RGB: AtomicU32 = AtomicU32::new(0x00A65327);

pub fn is_dark_theme() -> bool {
    SYSTEM_DARK_MODE.load(Ordering::Relaxed)
}

#[inline]
pub fn accent_rgb() -> u32 {
    ACCENT_RGB.load(Ordering::Relaxed)
}

#[inline]
pub fn accent_bgr() -> u32 {
    crate::render_aa::rgb_to_bgr(accent_rgb())
}

#[inline]
pub fn accent_light_rgb() -> u32 {
    ACCENT_LIGHT1_RGB.load(Ordering::Relaxed)
}

#[inline]
pub fn accent_light_bgr() -> u32 {
    crate::render_aa::rgb_to_bgr(accent_light_rgb())
}

#[inline]
pub fn accent_dark_bgr() -> u32 {
    crate::render_aa::rgb_to_bgr(ACCENT_DARK1_RGB.load(Ordering::Relaxed))
}

pub fn refresh_system_theme(hwnd: Option<HWND>) {
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let val_name = to_wide("AppsUseLightTheme");
    let mut val: u32 = 0;
    let mut size: u32 = 4;
    let dark = unsafe {
        if RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(val_name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut val as *mut u32 as *mut _),
            Some(&mut size),
        ) == ERROR_SUCCESS
        {
            val == 0
        } else {
            true
        }
    };
    SYSTEM_DARK_MODE.store(dark, Ordering::Relaxed);
    refresh_accent_palette();

    if let Some(h) = hwnd {
        unsafe {
            let dm: i32 = if dark { 1 } else { 0 };
            let _ = DwmSetWindowAttribute(
                h,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dm as *const _ as _,
                4,
            );
        }
    }
}

fn refresh_accent_palette() {
    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Accent");
    let val_name = to_wide("AccentPalette");
    let mut buf = [0u8; 32];
    let mut size: u32 = 32;
    let ok = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(val_name.as_ptr()),
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr() as *mut _),
            Some(&mut size),
        ) == ERROR_SUCCESS
    };
    if ok && size >= 24 {
        let rgb_at = |idx: usize| -> u32 {
            let r = buf[idx * 4] as u32;
            let g = buf[idx * 4 + 1] as u32;
            let b = buf[idx * 4 + 2] as u32;
            (r << 16) | (g << 8) | b
        };
        ACCENT_LIGHT1_RGB.store(rgb_at(2), Ordering::Relaxed);
        ACCENT_RGB.store(rgb_at(3), Ordering::Relaxed);
        ACCENT_DARK1_RGB.store(rgb_at(4), Ordering::Relaxed);
    }
}

#[derive(Clone, Copy)]
pub struct ThemePalette {
    pub dark: bool,
    pub win_bg: u32,
    pub sidebar_bg: u32,
    pub sidebar_hover_bg: u32,
    pub card_bg: u32,
    pub card_border: u32,
    pub sub_card_bg: u32,
    pub sub_card_border: u32,
    pub divider: u32,
    pub input_bg: u32,
    pub input_border: u32,
    pub text_primary: u32,
    pub text_bright: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    pub text_dim: u32,
    pub btn_bg: u32,
    pub btn_hover_bg: u32,
    pub btn_hover_border: u32,
    pub btn_border: u32,
    pub status_ok: u32,
    pub status_warn: u32,
    pub status_err: u32,
    pub scrollbar_track: u32,
    pub scrollbar_thumb: u32,
    pub accent_text: u32,
}

#[inline]
pub fn theme() -> ThemePalette {
    if is_dark_theme() {
        ThemePalette {
            dark: true,
            win_bg: 0x001C1919,
            sidebar_bg: 0x001B1B1E,
            sidebar_hover_bg: 0x002B2B2E,
            card_bg: 0x00242020,
            card_border: 0x00332D2D,
            sub_card_bg: 0x001B1717,
            sub_card_border: 0x002F2828,
            divider: 0x002F2828,
            input_bg: 0x001E1B1B,
            input_border: 0x00454040,
            text_primary: 0x00F0F0F0,
            text_bright: 0x00FFFFFF,
            text_secondary: 0x00D0D0D0,
            text_muted: 0x00A0A0A0,
            text_dim: 0x00888080,
            btn_bg: 0x00202022,
            btn_hover_bg: 0x002B2B2E,
            btn_hover_border: 0x00666666,
            btn_border: 0x003D3D44,
            status_ok: 0x006CCB5F,
            status_warn: 0x0033B8FF,
            status_err: 0x005555EE,
            scrollbar_track: 0x00201C1C,
            scrollbar_thumb: 0x004A4444,
            accent_text: accent_light_bgr(),
        }
    } else {
        ThemePalette {
            dark: false,
            win_bg: 0x00F3F3F3,
            sidebar_bg: 0x00ECECEC,
            sidebar_hover_bg: 0x00E0E0E0,
            card_bg: 0x00FFFFFF,
            card_border: 0x00E5E5E5,
            sub_card_bg: 0x00F9F9F9,
            sub_card_border: 0x00E5E5E5,
            divider: 0x00E5E5E5,
            input_bg: 0x00FFFFFF,
            input_border: 0x00CCCCCC,
            text_primary: 0x001A1A1A,
            text_bright: 0x00111111,
            text_secondary: 0x003A3A3A,
            text_muted: 0x005C5C5C,
            text_dim: 0x00757575,
            btn_bg: 0x00FBFBFB,
            btn_hover_bg: 0x00F2F2F2,
            btn_hover_border: 0x00BDBDBD,
            btn_border: 0x00DCDCDC,
            status_ok: 0x00107C10,
            status_warn: 0x000078D4,
            status_err: 0x001B1BD1,
            scrollbar_track: 0x00EAEAEA,
            scrollbar_thumb: 0x00C2C2C2,
            accent_text: accent_dark_bgr(),
        }
    }
}
