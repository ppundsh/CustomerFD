//! Single-line text editing for the custom-drawn text boxes.
//!
//! Win11 behaviour (see `doc/ui-design.zh-TW.md` §8.5): focusing places the caret where you
//! click and never selects everything; dragging selects; only the selected characters are
//! highlighted; Shift+←/→, Home/End, Ctrl+A/C/X/V, Backspace/Delete; typing replaces the
//! selection; Enter applies, Esc leaves the box.

use crate::app::{AppState, EditState, Field, TIMER_CARET};
use crate::sys;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC, HDC, HFONT};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetCapture, VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::SetTimer;

pub const MAX_FIELD_CHARS: usize = 1024;

fn byte_idx(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(s.len())
}

pub fn selection(e: &EditState, len: usize) -> Option<(usize, usize)> {
    let a = e.anchor?.min(len);
    let c = e.caret.min(len);
    (a != c).then(|| (a.min(c), a.max(c)))
}

fn delete_selection(text: &mut String, e: &mut EditState) -> bool {
    let len = text.chars().count();
    if let Some((a, b)) = selection(e, len) {
        let (ba, bb) = (byte_idx(text, a), byte_idx(text, b));
        text.replace_range(ba..bb, "");
        e.caret = a;
        e.anchor = None;
        true
    } else {
        false
    }
}

/// Character index nearest to `rel_x` pixels from the start of `text`.
pub unsafe fn hit_char(hdc: HDC, font: HFONT, text: &str, rel_x: i32) -> usize {
    if rel_x <= 0 || text.is_empty() {
        return 0;
    }
    let mut prev = 0;
    for (ci, (b, ch)) in text.char_indices().enumerate() {
        let w = cfd_ui::gdi::text_width(hdc, &text[..b + ch.len_utf8()], font);
        if rel_x < (prev + w) / 2 {
            return ci;
        }
        prev = w;
    }
    text.chars().count()
}

/// Text inset inside a box (left padding).
pub fn text_left(dpi: u32) -> i32 {
    cfd_ui::scale(10, dpi)
}

/// Mouse down on a text box: focus it and place the caret.
pub unsafe fn mouse_down(st: &mut AppState, field: Field, x: i32) {
    let hwnd = st.hwnd;
    let text = st.field_text(field).to_string();
    let dpi = st.dpi;
    let e = st.edits.entry(field).or_default();
    let hdc = GetDC(hwnd);
    let font = cfd_ui::gdi::create_font_body(hdc, dpi);
    let rel = x - (e.box_rect.left + text_left(dpi)) + e.scroll_x;
    let pos = hit_char(hdc, font, &text, rel);
    let _ = windows::Win32::Graphics::Gdi::DeleteObject(font);
    ReleaseDC(hwnd, hdc);
    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
    let same_field = st.focused == Some(field);
    let e = st.edits.entry(field).or_default();
    if shift && same_field {
        if e.anchor.is_none() {
            e.anchor = Some(e.caret);
        }
    } else {
        e.anchor = Some(pos);
    }
    e.caret = pos;
    e.dragging = true;
    st.focused = Some(field);
    st.caret_visible = true;
    let _ = SetCapture(hwnd);
    let _ = SetTimer(hwnd, TIMER_CARET, 530, None);
}

/// Mouse drag while selecting.
pub unsafe fn mouse_drag(st: &mut AppState, x: i32) {
    let Some(field) = st.focused else { return };
    let dragging = st.edits.get(&field).map(|e| e.dragging).unwrap_or(false);
    if !dragging {
        return;
    }
    let text = st.field_text(field).to_string();
    let hwnd = st.hwnd;
    let dpi = st.dpi;
    let hdc = GetDC(hwnd);
    let font = cfd_ui::gdi::create_font_body(hdc, dpi);
    let e = st.edits.entry(field).or_default();
    let rel = x - (e.box_rect.left + text_left(dpi)) + e.scroll_x;
    e.caret = hit_char(hdc, font, &text, rel);
    let _ = windows::Win32::Graphics::Gdi::DeleteObject(font);
    ReleaseDC(hwnd, hdc);
}

pub fn mouse_up(st: &mut AppState) {
    if let Some(f) = st.focused {
        if let Some(e) = st.edits.get_mut(&f) {
            e.dragging = false;
            if e.anchor == Some(e.caret) {
                e.anchor = None;
            }
        }
    }
}

/// What a key press asks the window to do next.
pub enum KeyResult {
    Handled,
    NotHandled,
    /// Enter: run the field's default action.
    Submit(Field),
}

/// WM_KEYDOWN for the focused field.
pub unsafe fn key_down(st: &mut AppState, hwnd: HWND, vk: u32) -> KeyResult {
    let Some(field) = st.focused else { return KeyResult::NotHandled };
    let ctrl = GetKeyState(VK_CONTROL.0 as i32) < 0;
    let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
    let mut text = st.field_text(field).to_string();
    let mut e = st.edits.get(&field).cloned().unwrap_or_default();
    let len = text.chars().count();
    e.caret = e.caret.min(len);
    let mut changed = false;
    let move_to = |e: &mut EditState, pos: usize| {
        if shift {
            if e.anchor.is_none() {
                e.anchor = Some(e.caret);
            }
        } else {
            e.anchor = None;
        }
        e.caret = pos;
    };
    match vk {
        0x25 /* LEFT */ => {
            if !shift {
                if let Some((a, _)) = selection(&e, len) {
                    e.caret = a;
                    e.anchor = None;
                } else {
                    { let p = e.caret.saturating_sub(1); move_to(&mut e, p); }
                }
            } else {
                { let p = e.caret.saturating_sub(1); move_to(&mut e, p); }
            }
        }
        0x27 /* RIGHT */ => {
            if !shift {
                if let Some((_, b)) = selection(&e, len) {
                    e.caret = b;
                    e.anchor = None;
                } else {
                    { let p = (e.caret + 1).min(len); move_to(&mut e, p); }
                }
            } else {
                { let p = (e.caret + 1).min(len); move_to(&mut e, p); }
            }
        }
        0x24 /* HOME */ => move_to(&mut e, 0),
        0x23 /* END */ => move_to(&mut e, len),
        0x08 /* BACK */ => {
            if !delete_selection(&mut text, &mut e) && e.caret > 0 {
                let b0 = byte_idx(&text, e.caret - 1);
                let b1 = byte_idx(&text, e.caret);
                text.replace_range(b0..b1, "");
                e.caret -= 1;
            }
            changed = true;
        }
        0x2E /* DELETE */ => {
            if !delete_selection(&mut text, &mut e) && e.caret < len {
                let b0 = byte_idx(&text, e.caret);
                let b1 = byte_idx(&text, e.caret + 1);
                text.replace_range(b0..b1, "");
            }
            changed = true;
        }
        0x41 if ctrl /* A */ => {
            e.anchor = Some(0);
            e.caret = len;
        }
        0x43 | 0x58 if ctrl /* C / X */ => {
            if let Some((a, b)) = selection(&e, len) {
                let s: String = text.chars().skip(a).take(b - a).collect();
                sys::copy_to_clipboard(hwnd, &s);
                if vk == 0x58 {
                    delete_selection(&mut text, &mut e);
                    changed = true;
                }
            }
        }
        0x56 if ctrl /* V */ => {
            if let Some(clip) = sys::read_clipboard_text(hwnd) {
                delete_selection(&mut text, &mut e);
                insert_str(&mut text, &mut e, &clip);
                changed = true;
            }
        }
        0x0D /* RETURN */ => {
            st.edits.insert(field, e);
            return KeyResult::Submit(field);
        }
        0x1B /* ESCAPE */ => {
            st.edits.insert(field, e);
            st.unfocus();
            return KeyResult::Handled;
        }
        _ => return KeyResult::NotHandled,
    }
    if changed {
        *st.field_mut(field) = text;
    }
    st.edits.insert(field, e);
    st.caret_visible = true;
    KeyResult::Handled
}

fn insert_str(text: &mut String, e: &mut EditState, s: &str) {
    let clean: String = s.chars().filter(|c| !c.is_control()).collect();
    let room = MAX_FIELD_CHARS.saturating_sub(text.chars().count());
    let clean: String = clean.chars().take(room).collect();
    let b = byte_idx(text, e.caret);
    text.insert_str(b, &clean);
    e.caret += clean.chars().count();
    e.anchor = None;
}

/// WM_CHAR for the focused field (handles UTF-16 surrogate pairs from the IME).
pub fn char_input(st: &mut AppState, unit: u16, pending_high: &mut Option<u16>) -> bool {
    let Some(field) = st.focused else { return false };
    let ch = if (0xD800..0xDC00).contains(&unit) {
        *pending_high = Some(unit);
        return true;
    } else if (0xDC00..0xE000).contains(&unit) {
        match pending_high.take() {
            Some(hi) => char::decode_utf16([hi, unit]).next().and_then(|r| r.ok()),
            None => None,
        }
    } else {
        char::from_u32(unit as u32)
    };
    let Some(ch) = ch else { return true };
    if ch.is_control() {
        return false;
    }
    let mut text = st.field_text(field).to_string();
    let mut e = st.edits.get(&field).cloned().unwrap_or_default();
    e.caret = e.caret.min(text.chars().count());
    delete_selection(&mut text, &mut e);
    insert_str(&mut text, &mut e, &ch.to_string());
    *st.field_mut(field) = text;
    st.edits.insert(field, e);
    st.caret_visible = true;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_ordered_and_clamped() {
        let e = EditState { caret: 1, anchor: Some(9), ..Default::default() };
        assert_eq!(selection(&e, 5), Some((1, 5)));
        let e = EditState { caret: 2, anchor: Some(2), ..Default::default() };
        assert_eq!(selection(&e, 5), None);
    }

    #[test]
    fn insert_replaces_and_respects_utf8() {
        let mut t = "資料夾".to_string();
        let mut e = EditState { caret: 1, anchor: Some(3), ..Default::default() };
        delete_selection(&mut t, &mut e);
        insert_str(&mut t, &mut e, "料\n夾X");
        assert_eq!(t, "資料夾X");
        assert_eq!(e.caret, 4);
    }
}
