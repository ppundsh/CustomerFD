//! GDI helpers: fonts, text, rounded boxes.

use crate::wstr::to_wide;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EnumFontFamiliesExW,
    FillRect, GetTextExtentPoint32W, LineTo, MoveToEx, RoundRect, SelectObject, SetBkMode,
    SetTextColor, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_PITCH, DRAW_TEXT_FORMAT, DT_CALCRECT,
    DT_NOPREFIX, DT_SINGLELINE, FF_DONTCARE, FW_NORMAL, FW_SEMIBOLD, HDC, HFONT, LOGFONTW,
    PS_SOLID, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::UI::Controls::{
    DrawThemeTextEx, DTTOPTS, DTT_COMPOSITED, DTT_TEXTCOLOR, HTHEME,
};

/// Body text face; falls back to "Segoe UI" where Segoe UI Variable is missing (Windows 10).
pub const UI_FACE: &str = "Segoe UI Variable Text";
const UI_FACE_FALLBACK: &str = "Segoe UI";
/// Icon glyph face; Windows 10 ships the same code points as "Segoe MDL2 Assets".
pub const ICON_FACE: &str = "Segoe Fluent Icons";
const ICON_FACE_FALLBACK: &str = "Segoe MDL2 Assets";

unsafe extern "system" fn font_exists_cb(
    _lf: *const LOGFONTW,
    _tm: *const TEXTMETRICW,
    _ty: u32,
    found: windows::Win32::Foundation::LPARAM,
) -> i32 {
    *(found.0 as *mut bool) = true;
    0
}

/// True when a font family with this name is installed.
pub fn font_installed(hdc: HDC, face: &str) -> bool {
    let mut lf = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..Default::default()
    };
    let w: Vec<u16> = face.encode_utf16().collect();
    let n = w.len().min(31);
    lf.lfFaceName[..n].copy_from_slice(&w[..n]);
    let mut found = false;
    // SAFETY: the callback only writes the bool whose address we pass and outlives the call.
    unsafe {
        EnumFontFamiliesExW(
            hdc,
            &lf,
            Some(font_exists_cb),
            windows::Win32::Foundation::LPARAM(&mut found as *mut bool as isize),
            0,
        );
    }
    found
}

/// Creates a font of `pt` points at `dpi`.
pub fn create_font(face: &str, pt: f32, weight: i32, dpi: u32) -> HFONT {
    let height = -((pt * dpi as f32 / 72.0).round() as i32);
    let face_w = to_wide(face);
    // SAFETY: plain GDI object creation with a NUL-terminated face name.
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            0,
            0,
            CLEARTYPE_QUALITY.0 as u32,
            (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
            PCWSTR(face_w.as_ptr()),
        )
    }
}

static FACES: std::sync::OnceLock<(&'static str, &'static str)> = std::sync::OnceLock::new();

/// `(ui_face, icon_face)` actually installed on this system (resolved once).
pub fn faces(hdc: HDC) -> (&'static str, &'static str) {
    *FACES.get_or_init(|| {
        (
            if font_installed(hdc, UI_FACE) {
                UI_FACE
            } else {
                UI_FACE_FALLBACK
            },
            if font_installed(hdc, ICON_FACE) {
                ICON_FACE
            } else {
                ICON_FACE_FALLBACK
            },
        )
    })
}

/// 10 pt body font (caller deletes it).
pub fn create_font_body(hdc: HDC, dpi: u32) -> HFONT {
    create_font(faces(hdc).0, 10.0, FW_NORMAL.0 as i32, dpi)
}

/// The font set of one paint pass; deleted on drop.
pub struct Fonts {
    /// Page title, 18 pt Semibold.
    pub hero: HFONT,
    /// Window title / section title, 14 pt Semibold.
    pub title: HFONT,
    /// Card title, 11 pt Semibold.
    pub card: HFONT,
    /// Body, buttons, menus, 10 pt.
    pub body: HFONT,
    /// Captions, status, 9 pt.
    pub small: HFONT,
    /// Icon glyphs at body size (16 px at 96 DPI).
    pub icon: HFONT,
    /// Small icon glyphs (12 px at 96 DPI) — chevrons, clear button.
    pub icon_small: HFONT,
}

impl Fonts {
    pub fn new(hdc: HDC, dpi: u32) -> Self {
        let (ui, ic) = faces(hdc);
        let semi = FW_SEMIBOLD.0 as i32;
        let norm = FW_NORMAL.0 as i32;
        Self {
            hero: create_font(ui, 18.0, semi, dpi),
            title: create_font(ui, 14.0, semi, dpi),
            card: create_font(ui, 11.0, semi, dpi),
            body: create_font(ui, 10.0, norm, dpi),
            small: create_font(ui, 9.0, norm, dpi),
            icon: create_font(ic, 12.0, norm, dpi),
            icon_small: create_font(ic, 9.0, norm, dpi),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        // SAFETY: each font was created by `Fonts::new` and is deleted exactly once.
        unsafe {
            for f in [
                self.hero,
                self.title,
                self.card,
                self.body,
                self.small,
                self.icon,
                self.icon_small,
            ] {
                if !f.is_invalid() {
                    let _ = DeleteObject(f);
                }
            }
        }
    }
}

/// Plain GDI text (opaque surfaces only: GDI leaves alpha at 0).
pub unsafe fn draw_text(
    hdc: HDC,
    text: &str,
    rc: &RECT,
    color: u32,
    font: HFONT,
    flags: DRAW_TEXT_FORMAT,
) {
    if text.is_empty() {
        return;
    }
    let old = SelectObject(hdc, font);
    SetTextColor(hdc, COLORREF(color));
    SetBkMode(hdc, TRANSPARENT);
    let mut w: Vec<u16> = text.encode_utf16().collect();
    let mut r = *rc;
    DrawTextW(hdc, &mut w, &mut r, flags | DT_NOPREFIX);
    SelectObject(hdc, old);
}

/// Text with correct alpha on translucent (Mica) areas via `DrawThemeTextEx(DTT_COMPOSITED)`.
pub unsafe fn draw_text_composited(
    hdc: HDC,
    htheme: HTHEME,
    text: &str,
    rc: &RECT,
    color: u32,
    font: HFONT,
    flags: DRAW_TEXT_FORMAT,
) {
    if text.is_empty() {
        return;
    }
    let old = SelectObject(hdc, font);
    let w: Vec<u16> = text.encode_utf16().collect();
    let mut r = *rc;
    let mut done = false;
    if !htheme.is_invalid() {
        let opts = DTTOPTS {
            dwSize: std::mem::size_of::<DTTOPTS>() as u32,
            dwFlags: DTT_COMPOSITED | DTT_TEXTCOLOR,
            crText: COLORREF(color),
            ..Default::default()
        };
        done = DrawThemeTextEx(
            htheme,
            hdc,
            0,
            0,
            &w,
            flags | DT_NOPREFIX,
            &mut r,
            Some(&opts),
        )
        .is_ok();
    }
    SelectObject(hdc, old);
    if !done {
        draw_text(hdc, text, rc, color, font, flags);
    }
}

/// Width of a single line of text in `font`.
pub unsafe fn text_width(hdc: HDC, text: &str, font: HFONT) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let old = SelectObject(hdc, font);
    let w: Vec<u16> = text.encode_utf16().collect();
    let mut sz = SIZE::default();
    let _ = GetTextExtentPoint32W(hdc, &w, &mut sz);
    SelectObject(hdc, old);
    sz.cx
}

/// Height of word-wrapped text laid out in `width`.
pub unsafe fn text_height_wrapped(hdc: HDC, text: &str, font: HFONT, width: i32) -> i32 {
    if text.is_empty() {
        return 0;
    }
    let old = SelectObject(hdc, font);
    let mut w: Vec<u16> = text.encode_utf16().collect();
    let mut r = RECT {
        left: 0,
        top: 0,
        right: width.max(1),
        bottom: 0,
    };
    DrawTextW(
        hdc,
        &mut w,
        &mut r,
        DT_CALCRECT | DT_NOPREFIX | windows::Win32::Graphics::Gdi::DT_WORDBREAK,
    );
    SelectObject(hdc, old);
    r.bottom - r.top
}

/// Single-line text width via DT_CALCRECT (handles font fallback for glyphs).
pub unsafe fn text_width_calc(hdc: HDC, text: &str, font: HFONT) -> i32 {
    let old = SelectObject(hdc, font);
    let mut w: Vec<u16> = text.encode_utf16().collect();
    let mut r = RECT::default();
    DrawTextW(
        hdc,
        &mut w,
        &mut r,
        DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
    );
    SelectObject(hdc, old);
    r.right - r.left
}

/// GDI rounded rectangle. `diameter` is the ellipse diameter (visual radius = diameter / 2).
pub unsafe fn round_box(hdc: HDC, rc: &RECT, diameter: i32, fill: u32, border: u32) {
    let brush = CreateSolidBrush(COLORREF(fill));
    let pen = CreatePen(PS_SOLID, 1, COLORREF(border));
    let ob = SelectObject(hdc, brush);
    let op = SelectObject(hdc, pen);
    let _ = RoundRect(
        hdc, rc.left, rc.top, rc.right, rc.bottom, diameter, diameter,
    );
    SelectObject(hdc, ob);
    SelectObject(hdc, op);
    let _ = DeleteObject(brush);
    let _ = DeleteObject(pen);
}

pub unsafe fn fill_rect(hdc: HDC, rc: &RECT, color: u32) {
    let brush = CreateSolidBrush(COLORREF(color));
    FillRect(hdc, rc, brush);
    let _ = DeleteObject(brush);
}

pub unsafe fn hline(hdc: HDC, x1: i32, x2: i32, y: i32, color: u32) {
    let pen = CreatePen(PS_SOLID, 1, COLORREF(color));
    let op = SelectObject(hdc, pen);
    let _ = MoveToEx(hdc, x1, y, None);
    let _ = LineTo(hdc, x2, y);
    SelectObject(hdc, op);
    let _ = DeleteObject(pen);
}

pub unsafe fn vline(hdc: HDC, x: i32, y1: i32, y2: i32, color: u32) {
    let pen = CreatePen(PS_SOLID, 1, COLORREF(color));
    let op = SelectObject(hdc, pen);
    let _ = MoveToEx(hdc, x, y1, None);
    let _ = LineTo(hdc, x, y2);
    SelectObject(hdc, op);
    let _ = DeleteObject(pen);
}

#[inline]
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

#[inline]
pub fn contains(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}
