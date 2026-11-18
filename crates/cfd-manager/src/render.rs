//! Paint pipeline: title bar, navigation rail, language flyout, content panel, modal dialog.
//!
//! Layering follows cbxshell-rs (`doc/ui-design.zh-TW.md` §4, §12): everything is drawn into a
//! 32-bit premultiplied DIB. The rail and title bar keep alpha 0 so DWM shows Mica; the content
//! panel is drawn with GDI, re-stamped opaque, and its plain page background is then made
//! ~70 % opaque so the same Mica glow shows through, darker.

#![allow(clippy::too_many_arguments)]

use crate::app::{Action, AppState, Cursor, ExpanderId, Field, Page, ToggleId};
use crate::edit;
use crate::i18n::tr;
use crate::pages;
use crate::updater;
use cfd_core::config::UiLang;
use cfd_ui::anim::{ease_out_cubic, ease_out_quart};
use cfd_ui::backdrop::{is_backdrop_active, CONTENT_MICA_TINT_ALPHA};
use cfd_ui::dib::DibSurface;
use cfd_ui::gdi::{self, draw_text, draw_text_composited, rect, round_box, Fonts};
use cfd_ui::render_aa::draw_semi_transparent_rect;
use cfd_ui::scale;
use cfd_ui::theme::{self, accent_bgr, accent_text_bgr, lerp_bgr, theme};
use cfd_ui::widgets::{self, ButtonKind};
use std::ffi::c_void;
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, EndPaint,
    IntersectClipRect, RestoreDC, SaveDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, DT_WORDBREAK, HDC, PAINTSTRUCT, SRCCOPY,
};
use windows::Win32::UI::Controls::{CloseThemeData, OpenThemeData, HTHEME};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

pub const RAIL_W: i32 = 195;
pub const RAIL_W_COLLAPSED: i32 = 60;
pub const TITLEBAR_H: i32 = 48;

pub const LEFT_VC: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0);
pub const CENTER_VC: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
pub const WRAP: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_WORDBREAK.0);

/// Rail collapse progress: 0 = expanded, 1 = collapsed.
pub fn rail_collapse(st: &AppState) -> f32 {
    let p = if st.nav_tween.is_active() { ease_out_cubic(st.nav_tween.raw()) } else { 1.0 };
    if st.nav_collapsed {
        p
    } else {
        1.0 - p
    }
}

pub fn rail_width(st: &AppState) -> i32 {
    let c = rail_collapse(st);
    let w = RAIL_W as f32 + (RAIL_W_COLLAPSED - RAIL_W) as f32 * c;
    scale(w.round() as i32, st.dpi)
}

/// Owns a top-down 32-bit DIB selected into a memory DC.
struct Surface {
    dc: HDC,
    bmp: windows::Win32::Graphics::Gdi::HBITMAP,
    old: windows::Win32::Graphics::Gdi::HGDIOBJ,
    dib: DibSurface,
}

impl Surface {
    unsafe fn new(ref_dc: HDC, w: i32, h: i32) -> Option<Self> {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(ref_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(bmp);
            return None;
        }
        std::ptr::write_bytes(bits.cast::<u8>(), 0, (w * h * 4) as usize);
        let dc = CreateCompatibleDC(ref_dc);
        let old = SelectObject(dc, bmp);
        Some(Self { dc, bmp, old, dib: DibSurface { bits: bits.cast(), w, h } })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: objects created in `Surface::new`, released once.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bmp);
        }
    }
}

/// WM_PAINT.
pub unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    let tb = cfd_ui::titlebar::titlebar_h(hwnd, TITLEBAR_H);
    if w > 0 && h > tb {
        let htheme = OpenThemeData(hwnd, w!("WINDOW"));
        crate::app::with_state(|st| {
            if let (Some(title), Some(body)) = (Surface::new(hdc, w, tb), Surface::new(hdc, w, h - tb)) {
                let fonts = Fonts::new(body.dc, st.dpi);
                render_title(st, &title, &fonts, htheme);
                render_body(st, &body, &fonts, htheme);
                let _ = BitBlt(hdc, 0, 0, w, tb, title.dc, 0, 0, SRCCOPY);
                let _ = BitBlt(hdc, 0, tb, w, h - tb, body.dc, 0, 0, SRCCOPY);
            }
        });
        if !htheme.is_invalid() {
            let _ = CloseThemeData(htheme);
        }
    }
    let _ = EndPaint(hwnd, &ps);
}

unsafe fn render_title(st: &AppState, s: &Surface, fonts: &Fonts, htheme: HTHEME) {
    let th = theme();
    let title = tr("CustomerFD 控制中心", "CustomerFD Control Center");
    let col = if th.dark { 0x00F0F0F0 } else { 0x001A1A1A };
    draw_text_composited(s.dc, htheme, title, &rect(scale(20, st.dpi), 0, s.dib.w / 2, s.dib.h), col, fonts.title, LEFT_VC);
    if !is_backdrop_active() {
        s.dib.flatten_onto_opaque(th.sidebar_bg);
    }
}

unsafe fn render_body(st: &mut AppState, s: &Surface, fonts: &Fonts, htheme: HTHEME) {
    let th = theme();
    let dpi = st.dpi;
    let (w, h) = (s.dib.w, s.dib.h);
    let rail_w = rail_width(st);
    let radius = scale(10, dpi);
    st.regions.clear();
    st.block_input = st.modal.is_some() || st.lang_open;

    // 1. content panel + page
    s.dib.fill_content_panel_aa(rail_w, radius, th.win_bg, th.card_border);
    let saved = SaveDC(s.dc);
    let _ = IntersectClipRect(s.dc, rail_w + 1, 1, w, h);
    let page_rc = RECT { left: rail_w + 1, top: 0, right: w, bottom: h };
    let trans = if st.page_tween.is_active() { ease_out_cubic(st.page_tween.raw()) } else { 1.0 };
    let dy = (scale(14, dpi) as f32 * (1.0 - trans)).round() as i32;
    let content_h = pages::render_page(st, s.dc, fonts, rail_w, w, h, dy);
    let _ = RestoreDC(s.dc, saved);
    st.view_h = h;
    st.max_scroll = (content_h - h).max(0);
    if st.scroll_y > st.max_scroll {
        st.scroll_y = st.max_scroll;
    }
    render_scrollbar(st, s.dc, w, h);
    s.dib.stamp_content_panel_alpha(rail_w, radius);
    if trans < 1.0 {
        s.dib.fade_rect_toward(&page_rc, th.win_bg, (trans * 255.0).round() as u8);
    }

    // 2. rail (translucent) + flyout
    let block = st.block_input;
    st.block_input = st.modal.is_some() || st.lang_open;
    render_rail(st, s, fonts, htheme, rail_w, h);
    st.block_input = block;
    if st.lang_open {
        st.block_input = st.modal.is_some();
        render_lang_flyout(st, s, fonts, htheme, rail_w, h);
    }
    if is_backdrop_active() {
        s.dib.tint_content_background(rail_w, th.win_bg, CONTENT_MICA_TINT_ALPHA);
    }

    // 3. modal
    if st.modal.is_some() {
        st.block_input = false;
        render_modal(st, s, fonts, w, h);
    }
    if !is_backdrop_active() {
        s.dib.flatten_onto_opaque(th.sidebar_bg);
    }
}

/// Translucent white (dark) / black (light) overlay alpha used by rail pills.
fn rail_overlay(alpha_dark: u8, alpha_light: u8) -> (u32, u8) {
    if theme::is_dark() {
        (0x00FF_FFFF, alpha_dark)
    } else {
        (0, alpha_light)
    }
}

unsafe fn rail_pill(s: &Surface, rc: &RECT, dpi: u32, hovered: bool, pressed: bool, selected: bool, press_p: f32) {
    let (rest_d, rest_l) = if selected { (16u8, 11u8) } else { (0, 0) };
    let (hov_d, hov_l) = if selected { (10u8, 7u8) } else { (16, 11) };
    let (mut ad, mut al) = if hovered { (hov_d, hov_l) } else { (rest_d, rest_l) };
    if pressed {
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * press_p).round() as u8;
        ad = lerp(ad, 10);
        al = lerp(al, 7);
    }
    let (col, a) = rail_overlay(ad, al);
    if a > 0 {
        s.dib.fill_rounded_translucent(rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top, scale(8, dpi), col, a, 0, 0);
    }
}

fn rail_text_color(pressed: bool, press_p: f32) -> u32 {
    let dark = theme::is_dark();
    let (base, dim) = if dark { (0x00F0F0F0, 0x00A6A6A6) } else { (0x001A1A1A, 0x00666666) };
    if pressed {
        lerp_bgr(base, dim, press_p)
    } else {
        base
    }
}

unsafe fn render_rail(st: &mut AppState, s: &Surface, fonts: &Fonts, htheme: HTHEME, rail_w: i32, h: i32) {
    let dpi = st.dpi;
    let collapse = rail_collapse(st);
    let pad = scale(10, dpi);
    let icon_col = scale(40, dpi);
    let press_p = ease_out_cubic(st.nav_press.raw());

    // Nav toggle (hamburger)
    let tog = rect(pad, scale(2, dpi), icon_col, scale(36, dpi));
    let hot = st.interact(tog, Action::NavToggle, Cursor::Hand);
    rail_pill(s, &tog, dpi, hot.hovered, hot.pressed, false, press_p);
    draw_text_composited(s.dc, htheme, widgets::GLYPH_NAV, &tog, rail_text_color(hot.pressed, press_p), fonts.icon, CENTER_VC);

    // Version / status next to the toggle
    if collapse < 0.35 {
        let (status, col) = rail_status(st);
        let x = tog.right + scale(8, dpi);
        let line = format!("v{}  ·  {status}", env!("CARGO_PKG_VERSION"));
        draw_text_composited(s.dc, htheme, &line, &RECT { left: x, top: tog.top, right: rail_w - pad, bottom: tog.bottom }, col, fonts.small, LEFT_VC);
    }

    // Nav items + selection indicator
    let item_top = |i: usize| scale(48 + 44 * i as i32, dpi);
    let item_h = scale(40, dpi);
    for p in Page::ALL {
        let i = p.index();
        let rc = RECT { left: pad, top: item_top(i), right: rail_w - pad, bottom: item_top(i) + item_h };
        let hot = st.interact(rc, Action::SwitchPage(p), Cursor::Hand);
        rail_pill(s, &rc, dpi, hot.hovered, hot.pressed, st.page == p, press_p);
        let col = rail_text_color(hot.pressed, press_p);
        draw_text_composited(s.dc, htheme, p.glyph(), &rect(pad, rc.top, icon_col, item_h), col, fonts.icon, CENTER_VC);
        if collapse < 0.5 {
            let lx = pad + icon_col + scale(4, dpi);
            draw_text_composited(s.dc, htheme, p.label(), &RECT { left: lx, top: rc.top, right: rc.right - scale(4, dpi), bottom: rc.bottom }, col, fonts.body, LEFT_VC);
        }
    }
    // indicator: 3 × 20 accent capsule, moves 220 ms with a 45 % squeeze half-way
    let to = st.page.index() as f32;
    let (pos, squeeze) = if st.indicator_tween.is_active() {
        let raw = st.indicator_tween.raw();
        let e = ease_out_cubic(raw);
        (st.indicator_from as f32 + (to - st.indicator_from as f32) * e, 1.0 - 0.45 * (std::f32::consts::PI * raw).sin())
    } else {
        (to, 1.0)
    };
    let ind_h = (scale(20, dpi) as f32 * squeeze).round().max(2.0) as i32;
    let center = scale(48, dpi) as f32 + scale(44, dpi) as f32 * pos + item_h as f32 / 2.0;
    let ab = accent_bgr();
    s.dib.fill_rounded_translucent(pad + scale(3, dpi), (center - ind_h as f32 / 2.0).round() as i32, scale(3, dpi), ind_h, scale(2, dpi), ab, 255, 0, 0);

    // Language button (bottom)
    let lang_rc = RECT { left: pad, top: h - scale(50, dpi), right: rail_w - pad, bottom: h - scale(10, dpi) };
    let hot = st.interact(lang_rc, Action::LangButton, Cursor::Hand);
    let open = st.lang_open;
    rail_pill(s, &lang_rc, dpi, hot.hovered || open, hot.pressed, false, press_p);
    let col = rail_text_color(hot.pressed, press_p);
    draw_text_composited(s.dc, htheme, widgets::GLYPH_GLOBE, &rect(pad, lang_rc.top, icon_col, lang_rc.bottom - lang_rc.top), col, fonts.icon, CENTER_VC);
    if collapse < 0.5 {
        let lx = pad + icon_col + scale(4, dpi);
        let name = lang_name(crate::i18n::get_lang());
        draw_text_composited(s.dc, htheme, name, &RECT { left: lx, top: lang_rc.top, right: lang_rc.right - scale(30, dpi), bottom: lang_rc.bottom }, col, fonts.body, LEFT_VC);
        let lift = if open || hot.pressed { scale(2, dpi) } else { 0 };
        let chev = rect(lang_rc.right - scale(30, dpi), lang_rc.top - lift, scale(24, dpi), lang_rc.bottom - lang_rc.top);
        draw_text_composited(s.dc, htheme, widgets::GLYPH_CHEVRON_UP, &chev, col, fonts.icon_small, CENTER_VC);
    }
}

fn lang_name(l: UiLang) -> &'static str {
    match l {
        UiLang::ZhTw => "繁體中文",
        UiLang::En => "English",
    }
}

/// Short status for the rail: update available > not applied > enabled.
pub fn rail_status(st: &AppState) -> (String, u32) {
    let th = theme();
    let muted = if th.dark { 0x00B4B4B4 } else { 0x00505050 };
    if let updater::Phase::Available(m) = updater::phase() {
        return (format!("{} {}", tr("有新版本", "Update"), m.version), accent_text_bgr());
    }
    if !st.registered {
        return (tr("尚未套用", "Not applied").to_string(), th.status_warn);
    }
    if st.is_dirty() {
        return (tr("有未套用變更", "Unapplied changes").to_string(), th.status_warn);
    }
    (tr("已啟用", "Active").to_string(), muted)
}

unsafe fn render_lang_flyout(st: &mut AppState, s: &Surface, fonts: &Fonts, htheme: HTHEME, _rail_w: i32, h: i32) {
    let dpi = st.dpi;
    let langs = [UiLang::ZhTw, UiLang::En];
    let cur = crate::i18n::get_lang();
    let sel = langs.iter().position(|l| *l == cur).unwrap_or(0) as i32;
    let item_h = scale(36, dpi);
    let pad = scale(4, dpi);
    let fw = scale(160, dpi);
    let fh = item_h * langs.len() as i32 + pad * 2;
    let anchor_top = h - scale(50, dpi);
    // ComboBox-style: the selected row sits on the button, clamped into the window.
    let mut top = anchor_top - pad - sel * item_h;
    top = top.clamp(scale(8, dpi), h - scale(8, dpi) - fh);
    let left = scale(10, dpi);

    let raw = st.lang_tween.raw();
    let p = if st.lang_open { ease_out_quart(raw) } else { 1.0 - ease_out_quart(raw) };
    if p <= 0.0 {
        return;
    }
    let sel_center = top + pad + sel * item_h + item_h / 2;
    let vis_top = sel_center - ((sel_center - top) as f32 * p).round() as i32;
    let vis_bot = sel_center + ((top + fh - sel_center) as f32 * p).round() as i32;

    // in-app acrylic: blur what is below, then tint and border
    let r = scale(8, dpi);
    s.dib.blur_round_rect(left, vis_top, left + fw, vis_bot, r, scale(12, dpi));
    let dark = theme::is_dark();
    let tint = if dark { 0x002C2C2C } else { 0x00F9F9F9 };
    s.dib.fill_rounded_translucent(left, vis_top, fw, vis_bot - vis_top, r, tint, 0xD8, theme().btn_border, 255);

    let saved = SaveDC(s.dc);
    let _ = IntersectClipRect(s.dc, left, vis_top, left + fw, vis_bot);
    // Clicks outside the menu close it: a full-window blocker under the items.
    st.regions.push(crate::app::Region { rect: rect(0, 0, s.dib.w, s.dib.h), action: Action::LangButton, cursor: Cursor::Arrow });
    let press_p = ease_out_cubic(st.lang_press.raw());
    for (i, l) in langs.iter().enumerate() {
        let rc = RECT { left: left + pad, top: top + pad + i as i32 * item_h, right: left + fw - pad, bottom: top + pad + (i as i32 + 1) * item_h };
        let hot = st.interact(rc, Action::SetLang(*l), Cursor::Hand);
        let selected = *l == cur;
        let (ad, al) = if hot.pressed { (9u8, 6u8) } else if hot.hovered || selected { (16, 11) } else { (0, 0) };
        let (col, a) = rail_overlay(ad, al);
        if a > 0 && rc.bottom > vis_top && rc.top < vis_bot {
            s.dib.fill_rounded_translucent(rc.left, rc.top.max(vis_top), rc.right - rc.left, rc.bottom.min(vis_bot) - rc.top.max(vis_top), scale(4, dpi), col, a, 0, 0);
        }
        if selected {
            let shrink = if hot.pressed { 1.0 - 0.4 * press_p } else { 1.0 };
            let ih = (scale(16, dpi) as f32 * shrink).round() as i32;
            let cy = (rc.top + rc.bottom) / 2;
            if cy - ih / 2 >= vis_top && cy + ih / 2 <= vis_bot {
                s.dib.fill_rounded_translucent(rc.left + scale(2, dpi), cy - ih / 2, scale(3, dpi), ih, scale(2, dpi), accent_bgr(), 255, 0, 0);
            }
        }
        let th = theme();
        let tc = if hot.pressed { th.text_secondary } else { th.text_primary };
        draw_text_composited(s.dc, htheme, lang_name(*l), &RECT { left: rc.left + scale(14, dpi), top: rc.top, right: rc.right, bottom: rc.bottom }, tc, fonts.body, LEFT_VC);
    }
    let _ = RestoreDC(s.dc, saved);
}

unsafe fn render_scrollbar(st: &mut AppState, hdc: HDC, w: i32, h: i32) {
    if st.max_scroll <= 0 {
        st.scroll_track = RECT::default();
        return;
    }
    let dpi = st.dpi;
    let track = RECT { left: w - scale(14, dpi), top: scale(6, dpi), right: w - scale(4, dpi), bottom: h - scale(6, dpi) };
    st.scroll_track = track;
    let thumb = widgets::scrollbar(hdc, dpi, &track, st.scroll_y, st.max_scroll, st.thumb_drag.is_some());
    st.interact(thumb, Action::ScrollThumb, Cursor::Arrow);
}

unsafe fn render_modal(st: &mut AppState, s: &Surface, fonts: &Fonts, w: i32, h: i32) {
    let dpi = st.dpi;
    let th = theme();
    let Some(m) = st.modal.as_ref() else { return };
    let (title, body, ok_label, danger) = (m.title.clone(), m.body.clone(), m.ok_label.clone(), m.danger);
    draw_semi_transparent_rect(s.dc, 0, 0, w, h, 0, if th.dark { 0.65 } else { 0.35 });
    st.regions.push(crate::app::Region { rect: rect(0, 0, w, h), action: Action::Block, cursor: Cursor::Arrow });

    let mw = scale(460, dpi).min(w - scale(32, dpi));
    let inner = mw - scale(48, dpi);
    let body_h = gdi::text_height_wrapped(s.dc, &body, fonts.body, inner);
    let footer_h = scale(80, dpi);
    let mh = (scale(24 + 32 + 12, dpi) + body_h + scale(24, dpi) + footer_h).min(h - scale(32, dpi));
    let x = (w - mw) / 2;
    let y = (h - mh) / 2;
    let card = rect(x, y, mw, mh);
    widgets::modal_shadow(s.dc, &card, scale(8, dpi));
    round_box(s.dc, &card, scale(8, dpi), th.card_bg, th.card_border);
    // footer strip
    let foot = RECT { left: x + 1, top: y + mh - footer_h, right: x + mw - 1, bottom: y + mh - 1 };
    gdi::fill_rect(s.dc, &foot, th.sub_card_bg);
    gdi::hline(s.dc, x + 1, x + mw - 1, foot.top, th.card_border);

    let tx = x + scale(24, dpi);
    let mut ty = y + scale(24, dpi);
    if danger {
        widgets::glyph(s.dc, fonts.icon, widgets::GLYPH_WARNING, &rect(tx, ty, scale(24, dpi), scale(32, dpi)), th.status_warn);
        draw_text(s.dc, &title, &rect(tx + scale(30, dpi), ty, inner - scale(30, dpi), scale(32, dpi)), th.text_bright, fonts.title, LEFT_VC);
    } else {
        draw_text(s.dc, &title, &rect(tx, ty, inner, scale(32, dpi)), th.text_bright, fonts.title, LEFT_VC);
    }
    ty += scale(32 + 12, dpi);
    draw_text(s.dc, &body, &rect(tx, ty, inner, body_h + scale(4, dpi)), th.text_secondary, fonts.body, WRAP);

    let bh = scale(32, dpi);
    let gap = scale(8, dpi);
    let bw = (inner - gap) / 2;
    let by = foot.top + (footer_h - bh) / 2;
    let ok_rc = rect(tx, by, bw, bh);
    let cancel_rc = rect(tx + bw + gap, by, bw, bh);
    let hot = st.interact(ok_rc, Action::ModalConfirm, Cursor::Hand);
    widgets::button(s.dc, fonts, dpi, &ok_rc, &ok_label, ButtonKind::Accent, hot.hovered, hot.pressed, false);
    let hot = st.interact(cancel_rc, Action::ModalCancel, Cursor::Hand);
    widgets::button(s.dc, fonts, dpi, &cancel_rc, tr("取消", "Cancel"), ButtonKind::Standard, hot.hovered, hot.pressed, false);
    // card + the offset shadow layers (they extend right / down only)
    s.dib.stamp_opaque_round_rect(card.left, card.top, card.right, card.bottom, scale(4, dpi));
    s.dib.stamp_opaque_rect(&RECT { left: card.left + 1, top: card.bottom, right: card.right + 6, bottom: card.bottom + 7 });
    s.dib.stamp_opaque_rect(&RECT { left: card.right, top: card.top + 1, right: card.right + 6, bottom: card.bottom });
}

// ---------------------------------------------------------------------------------------------
// Stateful widget wrappers used by the pages

/// Text box bound to `field`: frame, text with selection, caret, clear button.
pub unsafe fn textbox(st: &mut AppState, hdc: HDC, fonts: &Fonts, rc: RECT, field: Field, placeholder: &str) {
    let dpi = st.dpi;
    let th = theme();
    let focused = st.focused == Some(field);
    let hot = st.interact(rc, Action::FocusField(field), Cursor::IBeam);
    widgets::textbox_frame(hdc, dpi, &rc, focused, hot.hovered, false);
    let text = st.field_text(field).to_string();
    let len = text.chars().count();
    let show_clear = focused && !text.is_empty();
    let inner = RECT {
        left: rc.left + edit::text_left(dpi),
        top: rc.top,
        right: rc.right - if show_clear { widgets::textbox_clear_w(dpi) + scale(4, dpi) } else { scale(10, dpi) },
        bottom: rc.bottom - scale(2, dpi),
    };
    let mut e = st.edits.get(&field).cloned().unwrap_or_default();
    e.box_rect = rc;
    e.caret = e.caret.min(len);
    // keep the caret visible
    let caret_px = gdi::text_width(hdc, &text.chars().take(e.caret).collect::<String>(), fonts.body);
    let avail = inner.right - inner.left;
    if focused {
        if caret_px - e.scroll_x > avail - scale(2, dpi) {
            e.scroll_x = caret_px - avail + scale(2, dpi);
        } else if caret_px - e.scroll_x < 0 {
            e.scroll_x = caret_px;
        }
    } else {
        e.scroll_x = 0;
    }
    let total_w = gdi::text_width(hdc, &text, fonts.body);
    e.scroll_x = e.scroll_x.clamp(0, (total_w - avail + scale(2, dpi)).max(0));
    let saved = SaveDC(hdc);
    let _ = IntersectClipRect(hdc, inner.left, inner.top, inner.right, inner.bottom);
    let x0 = inner.left - e.scroll_x;
    if text.is_empty() {
        if !focused {
            draw_text(hdc, placeholder, &inner, th.text_dim, fonts.body, LEFT_VC);
        }
    } else {
        draw_text(hdc, &text, &RECT { left: x0, top: inner.top, right: x0 + total_w + scale(4, dpi), bottom: inner.bottom }, th.text_primary, fonts.body, CENTER_VC_LEFT);
        if focused {
            if let Some((a, b)) = edit::selection(&e, len) {
                let pre: String = text.chars().take(a).collect();
                let selt: String = text.chars().skip(a).take(b - a).collect();
                let sx = x0 + gdi::text_width(hdc, &pre, fonts.body);
                let sw = gdi::text_width(hdc, &selt, fonts.body);
                let hl = RECT { left: sx, top: inner.top + scale(7, dpi), right: sx + sw, bottom: inner.bottom - scale(5, dpi) };
                gdi::fill_rect(hdc, &hl, accent_bgr());
                draw_text(hdc, &selt, &RECT { left: sx, top: inner.top, right: sx + sw + scale(2, dpi), bottom: inner.bottom }, theme::accent_contrast_text_bgr(), fonts.body, CENTER_VC_LEFT);
            }
        }
    }
    if focused && st.caret_visible {
        let cx = x0 + caret_px;
        gdi::fill_rect(hdc, &RECT { left: cx, top: inner.top + scale(8, dpi), right: cx + 1.max(scale(1, dpi)), bottom: inner.bottom - scale(6, dpi) }, th.text_bright);
    }
    let _ = RestoreDC(hdc, saved);
    st.edits.insert(field, e);
    if show_clear {
        let crc = widgets::textbox_clear_rect(dpi, &rc);
        let hot = st.interact(crc, Action::ClearField(field), Cursor::Hand);
        widgets::textbox_clear_button(hdc, fonts, dpi, &crc, hot.hovered, hot.pressed);
    }
}

const CENTER_VC_LEFT: DRAW_TEXT_FORMAT = DRAW_TEXT_FORMAT(DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

/// Toggle switch bound to `id`, with its On/Off label, right edge at `right`, centred on `cy`.
pub unsafe fn toggle(st: &mut AppState, hdc: HDC, fonts: &Fonts, right: i32, cy: i32, id: ToggleId) -> RECT {
    let dpi = st.dpi;
    let (tw, thh) = (scale(40, dpi), scale(20, dpi));
    let rc = rect(right - tw, cy - thh / 2, tw, thh);
    let lw = scale(44, dpi);
    let hit = RECT { left: rc.left - lw - scale(8, dpi), top: rc.top - scale(8, dpi), right: rc.right + scale(4, dpi), bottom: rc.bottom + scale(8, dpi) };
    let hot = st.interact(hit, Action::Toggle(id), Cursor::Hand);
    let anim = st.toggles.get(&id).copied().unwrap_or(st.toggle_value(id) as u8 as f32);
    widgets::toggle(hdc, dpi, rc.left, rc.top, anim, hot.hovered, false);
    let th = theme();
    let label = if st.toggle_value(id) { tr("開啟", "On") } else { tr("關閉", "Off") };
    draw_text(
        hdc,
        label,
        &rect(rc.left - lw - scale(8, dpi), rc.top - scale(4, dpi), lw, thh + scale(8, dpi)),
        th.text_primary,
        fonts.body,
        DRAW_TEXT_FORMAT(DT_VCENTER.0 | DT_SINGLELINE.0 | DT_RIGHT.0),
    );
    rc
}

/// Standard / accent / disabled button bound to `action`.
pub unsafe fn button(st: &mut AppState, hdc: HDC, fonts: &Fonts, rc: RECT, label: &str, kind: ButtonKind, action: Action, disabled: bool) {
    let hot = if disabled { Default::default() } else { st.interact(rc, action, Cursor::Hand) };
    widgets::button(hdc, fonts, st.dpi, &rc, label, kind, hot.hovered, hot.pressed, disabled);
}

pub unsafe fn button_glyph(st: &mut AppState, hdc: HDC, fonts: &Fonts, rc: RECT, icon: &str, label: &str, kind: ButtonKind, action: Action, disabled: bool) {
    let hot = if disabled { Default::default() } else { st.interact(rc, action, Cursor::Hand) };
    widgets::button_with_glyph(hdc, fonts, st.dpi, &rc, icon, label, kind, hot.hovered, hot.pressed, disabled);
}

pub unsafe fn subtle(st: &mut AppState, hdc: HDC, fonts: &Fonts, rc: RECT, icon: Option<&str>, label: &str, action: Action, disabled: bool) {
    let hot = if disabled { Default::default() } else { st.interact(rc, action, Cursor::Hand) };
    widgets::subtle_button(hdc, fonts, st.dpi, &rc, icon, label, hot.hovered, hot.pressed, disabled);
}

/// Expander header (title + description + optional trailing text) and the card below it.
/// Returns `(card_rect, content_top, openness)`; the caller draws rows while `openness > 0`
/// and clips them to the card.
pub unsafe fn expander(
    st: &mut AppState,
    hdc: HDC,
    fonts: &Fonts,
    x: i32,
    y: i32,
    w: i32,
    id: ExpanderId,
    icon: &str,
    title: &str,
    desc: &str,
    content_h: i32,
) -> (RECT, i32, f32) {
    let dpi = st.dpi;
    let th = theme();
    let header_h = scale(68, dpi);
    let open = st.expanders.get(&id).map(|e| e.openness()).unwrap_or(0.0);
    let expanded = st.expanders.get(&id).map(|e| e.expanded).unwrap_or(false);
    let body_h = (content_h as f32 * open).round() as i32;
    let card_rc = rect(x, y, w, header_h + body_h);
    let header = rect(x, y, w, header_h);
    let hot = st.interact(header, Action::Expander(id), Cursor::Hand);
    widgets::expander_card(hdc, dpi, &card_rc, hot.hovered);
    let press_p = st.expanders.get(&id).map(|e| ease_out_cubic(e.press.raw())).unwrap_or(1.0);
    widgets::expander_header_fill(hdc, dpi, &card_rc, header_h, open > 0.0, hot.hovered, hot.pressed, press_p);
    let title_col = if hot.pressed { lerp_bgr(th.text_bright, th.text_muted, press_p) } else { th.text_bright };
    widgets::glyph(hdc, fonts.icon, icon, &rect(x + scale(16, dpi), y, scale(24, dpi), header_h), th.text_primary);
    let tx = x + scale(56, dpi);
    let tw = w - scale(56 + 60, dpi);
    draw_text(hdc, title, &rect(tx, y + scale(14, dpi), tw, scale(22, dpi)), title_col, fonts.body, LEFT_VC);
    draw_text(hdc, desc, &rect(tx, y + scale(35, dpi), tw, scale(20, dpi)), th.text_muted, fonts.small, LEFT_VC);
    widgets::chevron(hdc, fonts, dpi, x + w - scale(16 + 11, dpi), y + header_h / 2, expanded, th.text_primary);
    if open > 0.0 {
        widgets::expander_seam(hdc, dpi, x, y + header_h, w);
    }
    (card_rc, y + header_h, open)
}
