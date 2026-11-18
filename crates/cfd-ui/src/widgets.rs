//! Fluent controls drawn with GDI on an opaque surface (the content panel).
//!
//! Every function takes the interaction state explicitly (`hovered`, `pressed`, …) so the same
//! drawing code serves the control center and the installer. Measurements follow
//! `doc/ui-design.zh-TW.md` (cbxshell-rs) and are in 96-DPI design pixels, scaled by `dpi`.

#![allow(clippy::too_many_arguments)]

use crate::gdi::{draw_text, fill_rect, rect, round_box, Fonts};
use crate::render_aa::{draw_aa_checkbox, draw_aa_circle, draw_aa_pill, lerp_color_rgb};
use crate::scale;
use crate::theme::{
    accent_bgr, accent_contrast_text_bgr, accent_contrast_text_rgb, accent_hover_bgr,
    accent_pressed_bgr, accent_rgb, accent_text_bgr, lerp_bgr, theme,
};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, ExtSelectClipRgn, RestoreDC, SaveDC, DT_CENTER, DT_END_ELLIPSIS,
    DT_LEFT, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HDC, HFONT, RGN_AND,
};

// Segoe Fluent Icons / MDL2 code points.
pub const GLYPH_NAV: &str = "\u{E700}";
pub const GLYPH_CHEVRON_DOWN: &str = "\u{E70D}";
pub const GLYPH_CHEVRON_UP: &str = "\u{E70E}";
pub const GLYPH_ADD: &str = "\u{E710}";
pub const GLYPH_CANCEL: &str = "\u{E711}";
pub const GLYPH_SETTINGS: &str = "\u{E713}";
pub const GLYPH_REFRESH: &str = "\u{E72C}";
pub const GLYPH_UP: &str = "\u{E74A}";
pub const GLYPH_DOWN: &str = "\u{E74B}";
pub const GLYPH_DELETE: &str = "\u{E74D}";
pub const GLYPH_EDIT: &str = "\u{E70F}";
pub const GLYPH_GLOBE: &str = "\u{E774}";
pub const GLYPH_FOLDER: &str = "\u{E8B7}";
pub const GLYPH_FOLDER_OPEN: &str = "\u{E838}";
pub const GLYPH_NEW_FOLDER: &str = "\u{E8F4}";
pub const GLYPH_INFO: &str = "\u{E946}";
pub const GLYPH_CLEAR: &str = "\u{E894}";
pub const GLYPH_DOWNLOAD: &str = "\u{E896}";
pub const GLYPH_OPEN_EXTERNAL: &str = "\u{E8A7}";
pub const GLYPH_WARNING: &str = "\u{E7BA}";
pub const GLYPH_CHECK: &str = "\u{E73E}";
pub const GLYPH_COMPLETED: &str = "\u{E930}";
pub const GLYPH_ERROR: &str = "\u{E783}";
pub const GLYPH_IMAGE: &str = "\u{EB9F}";

const LEFT_VC: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT =
    windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(
        DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0,
    );
const CENTER_VC: windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT =
    windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

/// Card: `card_bg` with a 1 px border, visual radius 6 (no shadow; layering is by colour).
pub unsafe fn card(hdc: HDC, dpi: u32, rc: &RECT) {
    let th = theme();
    round_box(hdc, rc, scale(12, dpi), th.card_bg, th.card_border);
}

/// Expander card: grows 1 px on every side while its header is hovered.
pub unsafe fn expander_card(hdc: HDC, dpi: u32, rc: &RECT, hovered: bool) {
    let g = if hovered { scale(1, dpi).max(1) } else { 0 };
    card(
        hdc,
        dpi,
        &RECT {
            left: rc.left - g,
            top: rc.top - g,
            right: rc.right + g,
            bottom: rc.bottom + g,
        },
    );
}

/// Sub-card (one level deeper than a card), visual radius 3.
pub unsafe fn sub_card(hdc: HDC, dpi: u32, rc: &RECT) {
    let th = theme();
    round_box(hdc, rc, scale(6, dpi), th.sub_card_bg, th.sub_card_border);
}

/// Hover / press fill of an expander header (`expanded` → only the header strip is filled).
pub unsafe fn expander_header_fill(
    hdc: HDC,
    dpi: u32,
    card_rc: &RECT,
    header_h: i32,
    expanded: bool,
    hovered: bool,
    pressed: bool,
    press_anim: f32,
) {
    if !hovered && !pressed {
        return;
    }
    let th = theme();
    let hover_bg = if expanded {
        lerp_bgr(th.card_bg, th.btn_hover_bg, 0.65)
    } else {
        th.btn_hover_bg
    };
    let press_bg = lerp_bgr(th.card_bg, th.btn_hover_bg, 0.35);
    let fill = if pressed {
        lerp_bgr(
            if hovered { hover_bg } else { th.card_bg },
            press_bg,
            press_anim,
        )
    } else {
        hover_bg
    };
    let g = scale(1, dpi).max(1);
    let rc = RECT {
        left: card_rc.left - g,
        top: card_rc.top - g,
        right: card_rc.right + g,
        bottom: card_rc.bottom + g,
    };
    let d = scale(12, dpi);
    if !expanded {
        round_box(hdc, &rc, d, fill, th.card_border);
    } else {
        let saved = SaveDC(hdc);
        let rgn = CreateRectRgn(rc.left, rc.top, rc.right + 1, card_rc.top + header_h);
        let _ = ExtSelectClipRgn(hdc, rgn, RGN_AND);
        round_box(
            hdc,
            &RECT {
                left: rc.left,
                top: rc.top,
                right: rc.right,
                bottom: card_rc.top + header_h + d,
            },
            d,
            fill,
            th.card_border,
        );
        let _ = RestoreDC(hdc, saved);
        let _ = DeleteObject(rgn);
    }
}

/// 2 px seam in the page colour across a whole card (expander header / group separator).
pub unsafe fn expander_seam(hdc: HDC, dpi: u32, x: i32, y: i32, w: i32) {
    let h = scale(2, dpi).max(2);
    let g = scale(1, dpi).max(1);
    fill_rect(
        hdc,
        &RECT {
            left: x - g,
            top: y - h / 2,
            right: x + w + g,
            bottom: y - h / 2 + h,
        },
        theme().win_bg,
    );
}

/// Icon glyph centred in `rc`.
pub unsafe fn glyph(hdc: HDC, font: HFONT, ch: &str, rc: &RECT, color: u32) {
    draw_text(hdc, ch, rc, color, font, CENTER_VC);
}

/// Expander chevron: down when collapsed, up when expanded.
pub unsafe fn chevron(hdc: HDC, fonts: &Fonts, dpi: u32, cx: i32, cy: i32, up: bool, color: u32) {
    let sz = scale(16, dpi);
    glyph(
        hdc,
        fonts.icon_small,
        if up {
            GLYPH_CHEVRON_UP
        } else {
            GLYPH_CHEVRON_DOWN
        },
        &rect(cx - sz / 2, cy - sz / 2, sz, sz),
        color,
    );
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Standard bordered button.
    Standard,
    /// Accent-filled primary button.
    Accent,
}

/// Standard / accent button (radius 2.5, 10 pt centred text). Pressing only changes colour:
/// the label never moves and the border never thickens.
pub unsafe fn button(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    rc: &RECT,
    label: &str,
    kind: ButtonKind,
    hovered: bool,
    pressed: bool,
    disabled: bool,
) {
    let th = theme();
    let (bg, border, text) = if disabled {
        (th.btn_disabled_bg, th.btn_disabled_border, th.text_disabled)
    } else if kind == ButtonKind::Accent {
        let c = if pressed {
            accent_pressed_bgr()
        } else if hovered {
            accent_hover_bgr()
        } else {
            accent_bgr()
        };
        (c, c, accent_contrast_text_bgr())
    } else if pressed {
        (th.sub_card_bg, th.btn_border, th.text_secondary)
    } else if hovered {
        (th.btn_hover_bg, th.btn_hover_border, th.text_bright)
    } else {
        (th.btn_bg, th.btn_border, th.text_primary)
    };
    round_box(hdc, rc, scale(5, dpi), bg, border);
    draw_text(hdc, label, rc, text, fonts.body, CENTER_VC);
}

/// Button with a leading glyph.
pub unsafe fn button_with_glyph(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    rc: &RECT,
    icon: &str,
    label: &str,
    kind: ButtonKind,
    hovered: bool,
    pressed: bool,
    disabled: bool,
) {
    button(hdc, fonts, dpi, rc, "", kind, hovered, pressed, disabled);
    let th = theme();
    let color = if disabled {
        th.text_disabled
    } else if kind == ButtonKind::Accent {
        accent_contrast_text_bgr()
    } else if pressed {
        th.text_secondary
    } else if hovered {
        th.text_bright
    } else {
        th.text_primary
    };
    let gw = scale(16, dpi);
    let gap = scale(8, dpi);
    let tw = crate::gdi::text_width(hdc, label, fonts.body);
    let total = gw + gap + tw;
    let x0 = rc.left + ((rc.right - rc.left) - total).max(0) / 2;
    glyph(
        hdc,
        fonts.icon,
        icon,
        &RECT {
            left: x0,
            top: rc.top,
            right: x0 + gw,
            bottom: rc.bottom,
        },
        color,
    );
    draw_text(
        hdc,
        label,
        &RECT {
            left: x0 + gw + gap,
            top: rc.top,
            right: rc.right,
            bottom: rc.bottom,
        },
        color,
        fonts.body,
        LEFT_VC,
    );
}

/// Width a button needs for `label` (+ optional glyph) with Fluent padding.
pub unsafe fn button_width(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    label: &str,
    with_glyph: bool,
) -> i32 {
    let tw = crate::gdi::text_width(hdc, label, fonts.body);
    let extra = if with_glyph { scale(24, dpi) } else { 0 };
    (tw + extra + scale(32, dpi)).max(scale(72, dpi))
}

/// Subtle (borderless) command-bar background: transparent at rest, `btn_hover_bg` on hover /
/// while open, a darker mix while pressed; extends 2 px above and below, radius 4.
pub unsafe fn subtle_bg(hdc: HDC, dpi: u32, rc: &RECT, hovered: bool, pressed: bool, open: bool) {
    let th = theme();
    let fill = if pressed {
        Some(lerp_bgr(th.card_bg, th.btn_hover_bg, 0.55))
    } else if hovered || open {
        Some(th.btn_hover_bg)
    } else {
        None
    };
    if let Some(c) = fill {
        let p = scale(2, dpi);
        round_box(
            hdc,
            &RECT {
                left: rc.left,
                top: rc.top - p,
                right: rc.right,
                bottom: rc.bottom + p,
            },
            scale(8, dpi),
            c,
            c,
        );
    }
}

fn subtle_text_color(hovered: bool, pressed: bool, disabled: bool) -> u32 {
    let th = theme();
    if disabled {
        th.text_disabled
    } else if pressed {
        th.text_secondary
    } else if hovered {
        th.text_bright
    } else {
        th.text_primary
    }
}

/// Subtle command button with optional glyph and label.
pub unsafe fn subtle_button(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    rc: &RECT,
    icon: Option<&str>,
    label: &str,
    hovered: bool,
    pressed: bool,
    disabled: bool,
) {
    if !disabled {
        subtle_bg(hdc, dpi, rc, hovered, pressed, false);
    }
    let color = subtle_text_color(hovered, pressed, disabled);
    match (icon, label.is_empty()) {
        (Some(g), true) => glyph(hdc, fonts.icon, g, rc, color),
        (Some(g), false) => {
            let gw = scale(16, dpi);
            let gap = scale(6, dpi);
            let tw = crate::gdi::text_width(hdc, label, fonts.body);
            let x0 = rc.left + ((rc.right - rc.left) - (gw + gap + tw)).max(0) / 2;
            glyph(
                hdc,
                fonts.icon,
                g,
                &RECT {
                    left: x0,
                    top: rc.top,
                    right: x0 + gw,
                    bottom: rc.bottom,
                },
                color,
            );
            draw_text(
                hdc,
                label,
                &RECT {
                    left: x0 + gw + gap,
                    top: rc.top,
                    right: rc.right,
                    bottom: rc.bottom,
                },
                color,
                fonts.body,
                LEFT_VC,
            );
        }
        (None, _) => draw_text(hdc, label, rc, color, fonts.body, CENTER_VC),
    }
}

/// Segmented / selectable button: selected uses accent colours.
pub unsafe fn selectable_button(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    rc: &RECT,
    icon: Option<&str>,
    label: &str,
    selected: bool,
    hovered: bool,
    pressed: bool,
) {
    let th = theme();
    let (bg, border, text) = if selected {
        let c = if pressed {
            accent_pressed_bgr()
        } else if hovered {
            accent_hover_bgr()
        } else {
            accent_bgr()
        };
        (c, c, accent_contrast_text_bgr())
    } else if pressed {
        (th.sub_card_bg, th.btn_border, th.text_secondary)
    } else if hovered {
        (
            th.sel_unselected_hover_bg,
            th.btn_hover_border,
            th.text_bright,
        )
    } else {
        (
            th.sel_unselected_bg,
            th.sel_unselected_border,
            th.sel_unselected_text,
        )
    };
    round_box(hdc, rc, scale(5, dpi), bg, border);
    let pad = scale(10, dpi);
    let mut x = rc.left + pad;
    if let Some(g) = icon {
        let gw = scale(16, dpi);
        glyph(
            hdc,
            fonts.icon,
            g,
            &RECT {
                left: x,
                top: rc.top,
                right: x + gw,
                bottom: rc.bottom,
            },
            text,
        );
        x += gw + scale(6, dpi);
    }
    draw_text(
        hdc,
        label,
        &RECT {
            left: x,
            top: rc.top,
            right: rc.right - pad,
            bottom: rc.bottom,
        },
        text,
        fonts.body,
        LEFT_VC,
    );
}

/// Toggle switch (40 × 20 capsule). `anim` 0 = off, 1 = on (interpolated by the caller).
pub unsafe fn toggle(
    hdc: HDC,
    dpi: u32,
    x: i32,
    y: i32,
    anim: f32,
    hovered: bool,
    disabled: bool,
) -> RECT {
    let w = scale(40, dpi);
    let h = scale(20, dpi);
    let dark = theme().dark;
    let a = anim.clamp(0.0, 1.0);
    let (fill, fill_a, border) = if disabled {
        if dark {
            (0x222226, 0.4, 0x444448)
        } else {
            (0xE6E6E6, 0.8, 0xCCCCCC)
        }
    } else {
        let off_fill = if dark { 0x222226 } else { 0xF5F5F5 };
        let off_border = if dark { 0x9A9A9E } else { 0x606060 };
        let fa = if dark { 0.5 + 0.5 * a } else { 1.0 };
        (
            lerp_color_rgb(off_fill, accent_rgb(), a),
            fa,
            lerp_color_rgb(off_border, accent_rgb(), a),
        )
    };
    draw_aa_pill(hdc, x, y, w, h, fill, fill_a, border, 1.0, 1.0);
    let r_min = scale(5, dpi) as f32;
    let r_max = scale(6, dpi) as f32 + 0.5;
    let base_r = r_min + (r_max - r_min) * a;
    let margin = scale(4, dpi) as f32;
    let travel = w as f32 - margin * 2.0 - base_r * 2.0;
    let cx = (x as f32 + margin + base_r + travel * a).round() as i32;
    let knob_r = base_r + if hovered && !disabled { 0.8 } else { 0.0 };
    let (knob, knob_a) = if disabled {
        if dark {
            (0x555558, 0.8)
        } else {
            (0x9E9E9E, 0.85)
        }
    } else {
        let off_k = if dark { 0xCECECE } else { 0x606060 };
        let on_k = if dark {
            0x222226
        } else {
            accent_contrast_text_rgb()
        };
        (lerp_color_rgb(off_k, on_k, a), 1.0)
    };
    draw_aa_circle(hdc, cx, y + h / 2, knob_r, knob, knob_a, 0, 0.0, 0.0);
    rect(x, y, w, h)
}

/// Checkbox with the left-to-right check animation (`progress` 0..1 while checking).
pub unsafe fn checkbox(
    hdc: HDC,
    x: i32,
    y: i32,
    size: i32,
    checked: bool,
    hovered: bool,
    disabled: bool,
    progress: f32,
) {
    draw_aa_checkbox(hdc, x, y, size, checked, hovered, disabled, progress);
}

/// Win11 TextBox chrome; colours derived from the card colour.
pub unsafe fn textbox_frame(
    hdc: HDC,
    dpi: u32,
    rc: &RECT,
    focused: bool,
    hovered: bool,
    disabled: bool,
) {
    let th = theme();
    let base = th.card_bg;
    let (white, black) = (0x00FF_FFFFu32, 0u32);
    let (fill, side, bottom) = if th.dark {
        let fill = if disabled {
            lerp_bgr(base, white, 0.03)
        } else if focused {
            lerp_bgr(base, black, 0.28)
        } else if hovered {
            lerp_bgr(base, white, 0.09)
        } else {
            lerp_bgr(base, white, 0.06)
        };
        (
            fill,
            lerp_bgr(base, white, 0.08),
            lerp_bgr(base, white, if disabled { 0.08 } else { 0.45 }),
        )
    } else {
        let fill = if disabled {
            lerp_bgr(base, black, 0.03)
        } else if focused {
            white
        } else if hovered {
            lerp_bgr(base, black, 0.035)
        } else {
            lerp_bgr(base, black, 0.015)
        };
        (
            fill,
            lerp_bgr(base, black, 0.07),
            lerp_bgr(base, black, if disabled { 0.07 } else { 0.45 }),
        )
    };
    round_box(hdc, rc, scale(8, dpi), fill, side);
    let (col, h) = if focused && !disabled {
        (accent_bgr(), scale(2, dpi).max(2))
    } else {
        (bottom, 1)
    };
    let inset = scale(3, dpi);
    fill_rect(
        hdc,
        &RECT {
            left: rc.left + inset,
            top: rc.bottom - h,
            right: rc.right - inset,
            bottom: rc.bottom,
        },
        col,
    );
}

/// Width reserved at the right of a text box for its clear button.
pub fn textbox_clear_w(dpi: u32) -> i32 {
    scale(24, dpi)
}

/// Rectangle of the clear button inside a text box.
pub fn textbox_clear_rect(dpi: u32, box_rc: &RECT) -> RECT {
    let pad = scale(3, dpi);
    let bw = textbox_clear_w(dpi) - pad;
    RECT {
        left: box_rc.right - pad - bw,
        top: box_rc.top + pad,
        right: box_rc.right - pad,
        bottom: box_rc.bottom - pad,
    }
}

/// Clear ("X") button of a focused, non-empty text box.
pub unsafe fn textbox_clear_button(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    rc: &RECT,
    hovered: bool,
    pressed: bool,
) {
    let th = theme();
    let base = if th.dark {
        lerp_bgr(th.card_bg, 0, 0.28)
    } else {
        0x00FF_FFFF
    };
    let fill = if pressed {
        Some(if th.dark {
            lerp_bgr(base, 0xFFFFFF, 0.04)
        } else {
            lerp_bgr(base, 0, 0.03)
        })
    } else if hovered {
        Some(if th.dark {
            lerp_bgr(base, 0xFFFFFF, 0.08)
        } else {
            lerp_bgr(base, 0, 0.06)
        })
    } else {
        None
    };
    if let Some(c) = fill {
        round_box(hdc, rc, scale(6, dpi), c, c);
    }
    glyph(
        hdc,
        fonts.icon_small,
        GLYPH_CLEAR,
        rc,
        if pressed {
            th.text_muted
        } else {
            th.text_secondary
        },
    );
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
}

/// Height of an info bar holding `message` at width `w`.
pub unsafe fn info_bar_height(hdc: HDC, fonts: &Fonts, dpi: u32, w: i32, message: &str) -> i32 {
    let text_w = w - scale(16 + 20 + 12 + 40, dpi);
    let th = crate::gdi::text_height_wrapped(hdc, message, fonts.body, text_w);
    (th + scale(20, dpi)).max(scale(40, dpi))
}

/// Win11 InfoBar: tinted card with a severity glyph, wrapped message and optional close button.
/// Returns the close button rectangle.
pub unsafe fn info_bar(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    rc: &RECT,
    severity: Severity,
    message: &str,
    close_hovered: bool,
    close_pressed: bool,
) -> RECT {
    let th = theme();
    let (bg, border, icon_col, icon) = match severity {
        Severity::Info => (
            crate::theme::accent_subtle_bg_bgr(),
            th.card_border,
            accent_text_bgr(),
            GLYPH_INFO,
        ),
        Severity::Success => (
            th.success_bg,
            th.success_border,
            th.status_ok,
            GLYPH_COMPLETED,
        ),
        Severity::Warning => (
            th.caution_bg,
            th.caution_border,
            th.status_warn,
            GLYPH_WARNING,
        ),
        Severity::Error => (th.caution_bg, th.caution_border, th.status_err, GLYPH_ERROR),
    };
    round_box(hdc, rc, scale(8, dpi), bg, border);
    let gx = rc.left + scale(14, dpi);
    let gw = scale(20, dpi);
    let top_line = rect(gx, rc.top + scale(10, dpi), gw, scale(20, dpi));
    glyph(hdc, fonts.icon, icon, &top_line, icon_col);
    let text_rc = RECT {
        left: gx + gw + scale(12, dpi),
        top: rc.top + scale(10, dpi),
        right: rc.right - scale(44, dpi),
        bottom: rc.bottom - scale(8, dpi),
    };
    let text_col = if matches!(severity, Severity::Warning | Severity::Error) && th.dark {
        th.caution_text
    } else {
        th.text_primary
    };
    draw_text(
        hdc,
        message,
        &text_rc,
        text_col,
        fonts.body,
        DT_LEFT | DT_WORDBREAK,
    );
    let close = rect(
        rc.right - scale(38, dpi),
        rc.top + scale(6, dpi),
        scale(30, dpi),
        scale(28, dpi),
    );
    subtle_bg(hdc, dpi, &close, close_hovered, close_pressed, false);
    glyph(
        hdc,
        fonts.icon_small,
        GLYPH_CANCEL,
        &close,
        subtle_text_color(close_hovered, close_pressed, false),
    );
    close
}

/// Multi-layer offset shadow below a modal dialog.
pub unsafe fn modal_shadow(hdc: HDC, rc: &RECT, diameter: i32) {
    let dark = theme().dark;
    let layers: [(i32, i32, i32, u32); 3] = if dark {
        [
            (3, 4, 2, 0x0013_1112),
            (3, 3, 1, 0x000E_0C0E),
            (2, 2, 0, 0x0008_0709),
        ]
    } else {
        [
            (3, 4, 2, 0x00C8_C8C8),
            (3, 3, 1, 0x00BE_BEBE),
            (2, 2, 0, 0x00B4_B4B4),
        ]
    };
    for (dx, dy, exp, col) in layers {
        round_box(
            hdc,
            &RECT {
                left: rc.left + dx - exp,
                top: rc.top + dy - exp,
                right: rc.right + dx + exp,
                bottom: rc.bottom + dy + exp,
            },
            diameter + exp,
            col,
            col,
        );
    }
}

/// Fluent scrollbar: track + thumb. Returns the thumb rectangle.
pub unsafe fn scrollbar(
    hdc: HDC,
    dpi: u32,
    track: &RECT,
    scroll: i32,
    max_scroll: i32,
    dragging: bool,
) -> RECT {
    let th = theme();
    round_box(hdc, track, scale(6, dpi), th.scrollbar_track, th.divider);
    let thumb = scrollbar_thumb_rect(dpi, track, scroll, max_scroll);
    let c = if dragging {
        th.text_dim
    } else {
        th.scrollbar_thumb
    };
    round_box(hdc, &thumb, scale(6, dpi), c, c);
    thumb
}

/// Thumb rectangle for a track (min thumb height 30).
pub fn scrollbar_thumb_rect(dpi: u32, track: &RECT, scroll: i32, max_scroll: i32) -> RECT {
    let track_h = track.bottom - track.top;
    let view = track_h.max(1) as f32;
    let content = view + max_scroll.max(0) as f32;
    let thumb_h = ((view / content) * track_h as f32)
        .round()
        .max(scale(30, dpi) as f32)
        .min(track_h as f32) as i32;
    let ratio = if max_scroll > 0 {
        scroll as f32 / max_scroll as f32
    } else {
        0.0
    };
    let y = track.top + ((track_h - thumb_h) as f32 * ratio.clamp(0.0, 1.0)).round() as i32;
    RECT {
        left: track.left + 1,
        top: y,
        right: track.right - 1,
        bottom: y + thumb_h,
    }
}

/// Status dot + label (e.g. "● 已啟用").
pub unsafe fn status_dot(
    hdc: HDC,
    fonts: &Fonts,
    dpi: u32,
    x: i32,
    cy: i32,
    color_bgr: u32,
    label: &str,
) -> i32 {
    let r = scale(4, dpi) as f32;
    draw_aa_circle(
        hdc,
        x + scale(4, dpi),
        cy,
        r,
        crate::theme::bgr_to_rgb(color_bgr),
        1.0,
        0,
        0.0,
        0.0,
    );
    let tx = x + scale(14, dpi);
    let tw = crate::gdi::text_width(hdc, label, fonts.small);
    draw_text(
        hdc,
        label,
        &rect(tx, cy - scale(10, dpi), tw + scale(4, dpi), scale(20, dpi)),
        color_bgr,
        fonts.small,
        LEFT_VC,
    );
    tx + tw - x
}
