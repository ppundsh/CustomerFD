//! Page content: sidebar settings, subfolders, about.
//!
//! Layout (96-DPI design px, see `doc/ui-design.zh-TW.md` §2): content starts at rail + 25, page
//! title at y = 25 (18 pt), first card at y = 75 (+ info bar), 14 between cards, one feature per
//! card; settings with sub-settings are expanders.

#![allow(clippy::too_many_arguments)]

use crate::app::{
    Action, AppState, Confirm, Cursor, ExpanderId, Field, Page, ToggleId, DEFAULT_ROOT_ICON,
};
use crate::i18n::tr;
use crate::render::{
    self, button, button_glyph, subtle, textbox, toggle, CENTER_VC, LEFT_VC, WRAP,
};
use crate::updater::{self, Phase};
use cfd_core::config::config_dir;
use cfd_core::registry::{find_shell_dll, registered_dll_path};
use cfd_ui::gdi::{self, draw_text, rect, round_box, text_height_wrapped, text_width, Fonts};
use cfd_ui::render_aa::{blend_bgra_to_hdc, draw_aa_pill};
use cfd_ui::scale;
use cfd_ui::theme::{accent_rgb, accent_text_bgr, bgr_to_rgb, theme};
use cfd_ui::widgets::{self, ButtonKind, Severity};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{IntersectClipRect, RestoreDC, SaveDC, HDC};
use windows::Win32::UI::WindowsAndMessaging::{DrawIconEx, DI_NORMAL};

/// Renders the active page; returns the total (unscrolled) content height.
pub unsafe fn render_page(
    st: &mut AppState,
    hdc: HDC,
    fonts: &Fonts,
    rail_w: i32,
    w: i32,
    _h: i32,
    dy: i32,
) -> i32 {
    let dpi = st.dpi;
    let th = theme();
    let x = rail_w + scale(25, dpi);
    let avail = w - x - scale(25 + 14, dpi);
    let cw = avail.min(scale(1000, dpi)).max(scale(300, dpi));
    let top = -st.scroll_y + dy;
    let mut y = top + scale(25, dpi);
    draw_text(
        hdc,
        page_title(st.page),
        &rect(x, y, cw, scale(36, dpi)),
        th.text_bright,
        fonts.hero,
        LEFT_VC,
    );
    y = top + scale(75, dpi);

    if let Some(info) = st.info.as_ref() {
        let (sev, msg) = (info.severity, info.message.clone());
        let ih = widgets::info_bar_height(hdc, fonts, dpi, cw, &msg);
        let rc = rect(x, y, cw, ih);
        // the close button's region is registered first-hit after the bar's
        let close_guess = rect(
            rc.right - scale(38, dpi),
            rc.top + scale(6, dpi),
            scale(30, dpi),
            scale(28, dpi),
        );
        let hot = st.interact(close_guess, Action::DismissInfo, Cursor::Hand);
        widgets::info_bar(hdc, fonts, dpi, &rc, sev, &msg, hot.hovered, hot.pressed);
        y += ih + scale(14, dpi);
    }

    y = match st.page {
        Page::Sidebar => page_sidebar(st, hdc, fonts, x, y, cw),
        Page::Subfolders => page_subfolders(st, hdc, fonts, x, y, cw),
        Page::About => page_about(st, hdc, fonts, x, y, cw),
    };
    y - top + scale(25, dpi)
}

fn page_title(p: Page) -> &'static str {
    match p {
        Page::Sidebar => tr("側邊欄設定", "Sidebar settings"),
        Page::Subfolders => tr("子資料夾", "Subfolders"),
        Page::About => tr("關於", "About"),
    }
}

/// Plain card with a glyph, title and description header. Returns the card rectangle.
unsafe fn card_header(
    st: &AppState,
    hdc: HDC,
    fonts: &Fonts,
    rc: &RECT,
    icon: &str,
    title: &str,
    desc: &str,
) {
    let dpi = st.dpi;
    let th = theme();
    widgets::card(hdc, dpi, rc);
    let hh = scale(68, dpi);
    widgets::glyph(
        hdc,
        fonts.icon,
        icon,
        &rect(rc.left + scale(16, dpi), rc.top, scale(24, dpi), hh),
        th.text_primary,
    );
    let tx = rc.left + scale(56, dpi);
    let tw = rc.right - tx - scale(16, dpi);
    draw_text(
        hdc,
        title,
        &rect(tx, rc.top + scale(14, dpi), tw, scale(22, dpi)),
        th.text_bright,
        fonts.body,
        LEFT_VC,
    );
    draw_text(
        hdc,
        desc,
        &rect(tx, rc.top + scale(35, dpi), tw, scale(20, dpi)),
        th.text_muted,
        fonts.small,
        LEFT_VC,
    );
}

/// Setting row card: header on the left, control area on the right (caller draws it).
/// Returns the vertical centre of the row.
unsafe fn row_card(
    st: &AppState,
    hdc: HDC,
    fonts: &Fonts,
    x: i32,
    y: i32,
    w: i32,
    icon: &str,
    title: &str,
    desc: &str,
    right_reserve: i32,
) -> i32 {
    let dpi = st.dpi;
    let th = theme();
    let h = scale(68, dpi);
    widgets::card(hdc, dpi, &rect(x, y, w, h));
    widgets::glyph(
        hdc,
        fonts.icon,
        icon,
        &rect(x + scale(16, dpi), y, scale(24, dpi), h),
        th.text_primary,
    );
    let tx = x + scale(56, dpi);
    let tw = w - scale(56 + 16, dpi) - right_reserve;
    draw_text(
        hdc,
        title,
        &rect(tx, y + scale(14, dpi), tw, scale(22, dpi)),
        th.text_bright,
        fonts.body,
        LEFT_VC,
    );
    draw_text(
        hdc,
        desc,
        &rect(tx, y + scale(35, dpi), tw, scale(20, dpi)),
        th.text_muted,
        fonts.small,
        LEFT_VC,
    );
    y + h / 2
}

/// Field label above a text box.
unsafe fn label(hdc: HDC, fonts: &Fonts, x: i32, y: i32, w: i32, dpi: u32, text: &str) {
    draw_text(
        hdc,
        text,
        &rect(x, y, w, scale(20, dpi)),
        theme().text_secondary,
        fonts.small,
        LEFT_VC,
    );
}

/// Wrapping row of root-folder selector buttons (+ add / delete when `manage`).
unsafe fn root_selector(
    st: &mut AppState,
    hdc: HDC,
    fonts: &Fonts,
    x: i32,
    y: i32,
    w: i32,
    manage: bool,
) -> i32 {
    let dpi = st.dpi;
    let bh = scale(32, dpi);
    let gap = scale(8, dpi);
    let mut cx = x;
    let mut cy = y;
    let roots: Vec<String> = st
        .config
        .root_folders
        .iter()
        .map(|r| r.name.clone())
        .collect();
    for (i, name) in roots.iter().enumerate() {
        let shown = if name.trim().is_empty() {
            tr("（未命名）", "(unnamed)")
        } else {
            name.as_str()
        };
        let bw = (text_width(hdc, shown, fonts.body) + scale(46, dpi))
            .clamp(scale(96, dpi), scale(260, dpi));
        if cx + bw > x + w && cx > x {
            cx = x;
            cy += bh + gap;
        }
        let rc = rect(cx, cy, bw, bh);
        let hot = st.interact(rc, Action::SelectRoot(i), Cursor::Hand);
        widgets::selectable_button(
            hdc,
            fonts,
            dpi,
            &rc,
            Some(widgets::GLYPH_FOLDER),
            shown,
            i == st.selected_root,
            hot.hovered,
            hot.pressed,
        );
        cx += bw + gap;
    }
    if manage {
        let add_label = tr("新增", "Add");
        let bw = widgets::button_width(hdc, fonts, dpi, add_label, true);
        if cx + bw > x + w && cx > x {
            cx = x;
            cy += bh + gap;
        }
        button_glyph(
            st,
            hdc,
            fonts,
            rect(cx, cy, bw, bh),
            widgets::GLYPH_ADD,
            add_label,
            ButtonKind::Standard,
            Action::AddRoot,
            false,
        );
        cx += bw + gap;
        let del_label = tr("刪除", "Delete");
        let dw = widgets::button_width(hdc, fonts, dpi, del_label, true);
        if cx + dw > x + w && cx > x {
            cx = x;
            cy += bh + gap;
        }
        let can_delete = st.config.root_folders.len() > 1;
        let sel = st.selected_root;
        subtle(
            st,
            hdc,
            fonts,
            rect(cx, cy, dw, bh),
            Some(widgets::GLYPH_DELETE),
            del_label,
            Action::Ask(Confirm::DeleteRoot(sel)),
            !can_delete,
        );
    }
    cy + bh
}

unsafe fn page_sidebar(
    st: &mut AppState,
    hdc: HDC,
    fonts: &Fonts,
    x: i32,
    mut y: i32,
    w: i32,
) -> i32 {
    let dpi = st.dpi;
    let th = theme();
    let gap = scale(14, dpi);
    let pad = scale(16, dpi);
    let inner_x = x + scale(56, dpi);
    let inner_w = w - scale(56, dpi) - pad;

    // 1. Root folders
    {
        let top = y;
        // measure first (selector height depends on wrapping), then draw card behind it
        let saved = SaveDC(hdc);
        let _ = IntersectClipRect(hdc, 0, 0, 0, 0);
        let regions_before = st.regions.len();
        let end = root_selector(st, hdc, fonts, inner_x, top + scale(68, dpi), inner_w, true);
        st.regions.truncate(regions_before);
        let _ = RestoreDC(hdc, saved);
        let rc = RECT {
            left: x,
            top,
            right: x + w,
            bottom: end + pad,
        };
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_FOLDER,
            tr("根資料夾", "Root folders"),
            tr(
                "每個根資料夾都是檔案總管側邊欄中的一個項目（最多 64 個）",
                "Each root folder is one entry in the Explorer sidebar (up to 64)",
            ),
        );
        root_selector(st, hdc, fonts, inner_x, top + scale(68, dpi), inner_w, true);
        y = rc.bottom + gap;
    }

    // 2. Name and content source
    {
        let top = y;
        let h = scale(68 + 20 + 34 + 12 + 20 + 34 + 20, dpi);
        let rc = rect(x, top, w, h);
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_EDIT,
            tr("名稱與內容來源", "Name and content source"),
            tr(
                "側邊欄顯示的名稱，以及要合併顯示的資料夾",
                "The name shown in the sidebar and an optional folder to show inside it",
            ),
        );
        let mut cy = top + scale(68, dpi);
        label(
            hdc,
            fonts,
            inner_x,
            cy,
            inner_w,
            dpi,
            tr("顯示名稱", "Display name"),
        );
        cy += scale(20, dpi);
        textbox(
            st,
            hdc,
            fonts,
            rect(inner_x, cy, inner_w, scale(32, dpi)),
            Field::RootName,
            tr("例如：工作區", "e.g. Workspace"),
        );
        cy += scale(34 + 12, dpi);
        label(hdc, fonts, inner_x, cy, inner_w, dpi, tr("內容來源資料夾（選用）：根資料夾會顯示這個資料夾的內容，子資料夾列在最前面", "Content folder (optional): the root shows this folder's contents, with your subfolders listed first"));
        cy += scale(20, dpi);
        let bw = widgets::button_width(hdc, fonts, dpi, tr("瀏覽...", "Browse..."), false);
        textbox(
            st,
            hdc,
            fonts,
            rect(inner_x, cy, inner_w - bw - scale(8, dpi), scale(32, dpi)),
            Field::RootPath,
            tr(
                "例如：D:\\Workspace 或 \\\\NAS\\Share",
                "e.g. D:\\Workspace or \\\\NAS\\Share",
            ),
        );
        button(
            st,
            hdc,
            fonts,
            rect(inner_x + inner_w - bw, cy, bw, scale(32, dpi)),
            tr("瀏覽...", "Browse..."),
            ButtonKind::Standard,
            Action::BrowseRootPath,
            false,
        );
        y = rc.bottom + gap;
    }

    // 3. Icon (expander)
    {
        let icon_loc = st.root().icon.clone();
        let icons = st.custom_icons.clone();
        let rows = icons.len().min(8) as i32;
        let content_h = scale(16 + 20 + 34 + 12 + 32 + 16, dpi)
            + if rows > 0 {
                scale(2 + 16 + 20 + 8, dpi) + rows * scale(40, dpi) + scale(8, dpi)
            } else {
                0
            };
        let desc = if icon_loc.trim().is_empty() || icon_loc.eq_ignore_ascii_case(DEFAULT_ROOT_ICON)
        {
            tr("預設資料夾圖示", "Default folder icon").to_string()
        } else {
            icon_loc.clone()
        };
        let (card_rc, ctop, open) = render::expander(
            st,
            hdc,
            fonts,
            x,
            y,
            w,
            ExpanderId::Icon,
            widgets::GLYPH_IMAGE,
            tr("圖示", "Icon"),
            &desc,
            content_h,
        );
        // current icon preview in the header, left of the chevron
        let isz = scale(24, dpi);
        if let Some(icon) = st.icons.get(&icon_loc, isz) {
            let _ = DrawIconEx(
                hdc,
                x + w - scale(16 + 22 + 16, dpi) - isz,
                y + (scale(68, dpi) - isz) / 2,
                icon,
                isz,
                isz,
                0,
                None,
                DI_NORMAL,
            );
        }
        if open > 0.0 {
            let saved = SaveDC(hdc);
            let _ = IntersectClipRect(
                hdc,
                card_rc.left,
                ctop + 1,
                card_rc.right,
                card_rc.bottom - 1,
            );
            let regions_before = st.regions.len();
            let mut cy = ctop + scale(16, dpi);
            let ix = x + scale(56, dpi);
            let iw = w - scale(56, dpi) - pad;
            label(
                hdc,
                fonts,
                ix,
                cy,
                iw,
                dpi,
                tr(
                    "圖示位置（.ico 檔，或 DLL／EXE 加上「,索引」）",
                    "Icon location (.ico file, or DLL/EXE with \",index\")",
                ),
            );
            cy += scale(20, dpi);
            let bw = widgets::button_width(hdc, fonts, dpi, tr("瀏覽...", "Browse..."), false);
            textbox(
                st,
                hdc,
                fonts,
                rect(ix, cy, iw - bw - scale(8, dpi), scale(32, dpi)),
                Field::RootIcon,
                DEFAULT_ROOT_ICON,
            );
            button(
                st,
                hdc,
                fonts,
                rect(ix + iw - bw, cy, bw, scale(32, dpi)),
                tr("瀏覽...", "Browse..."),
                ButtonKind::Standard,
                Action::BrowseIcon,
                false,
            );
            cy += scale(34 + 12, dpi);
            let l1 = tr("從圖片建立圖示", "Create icon from image");
            let w1 = widgets::button_width(hdc, fonts, dpi, l1, true);
            button_glyph(
                st,
                hdc,
                fonts,
                rect(ix, cy, w1, scale(32, dpi)),
                widgets::GLYPH_ADD,
                l1,
                ButtonKind::Standard,
                Action::ImportIcon,
                false,
            );
            let l2 = tr("還原預設圖示", "Use default icon");
            let w2 = widgets::button_width(hdc, fonts, dpi, l2, false);
            let is_default = icon_loc.eq_ignore_ascii_case(DEFAULT_ROOT_ICON);
            button(
                st,
                hdc,
                fonts,
                rect(ix + w1 + scale(8, dpi), cy, w2, scale(32, dpi)),
                l2,
                ButtonKind::Standard,
                Action::SelectIcon(DEFAULT_ROOT_ICON.to_string()),
                is_default,
            );
            cy += scale(32 + 16, dpi);
            if rows > 0 {
                widgets::expander_seam(hdc, dpi, x, cy, w);
                cy += scale(2 + 16, dpi);
                label(
                    hdc,
                    fonts,
                    ix,
                    cy,
                    iw,
                    dpi,
                    tr("已建立的自訂圖示", "Your custom icons"),
                );
                cy += scale(28, dpi);
                for p in icons.iter().take(8) {
                    let ps = p.display().to_string();
                    let selected = icon_loc.eq_ignore_ascii_case(&ps);
                    let row = rect(ix, cy, iw, scale(36, dpi));
                    let del_rc = rect(
                        row.right - scale(36, dpi),
                        row.top + scale(2, dpi),
                        scale(32, dpi),
                        scale(32, dpi),
                    );
                    let hot = st.interact(
                        RECT {
                            right: del_rc.left - scale(4, dpi),
                            ..row
                        },
                        Action::SelectIcon(ps.clone()),
                        Cursor::Hand,
                    );
                    let bg = if selected {
                        cfd_ui::theme::accent_subtle_bg_bgr()
                    } else if hot.hovered {
                        th.btn_hover_bg
                    } else {
                        th.sub_card_bg
                    };
                    round_box(
                        hdc,
                        &row,
                        scale(6, dpi),
                        bg,
                        if selected {
                            cfd_ui::theme::accent_bgr()
                        } else {
                            th.sub_card_border
                        },
                    );
                    let psz = scale(20, dpi) as u32;
                    let key = (p.clone(), psz);
                    if !st.icon_previews.contains_key(&key) {
                        let v = cfd_core::custom_icon::load_icon_preview_bgra(p, psz);
                        st.icon_previews.insert(key.clone(), v);
                    }
                    if let Some(Some(buf)) = st.icon_previews.get(&key) {
                        blend_bgra_to_hdc(
                            hdc,
                            row.left + scale(10, dpi),
                            row.top + (scale(36, dpi) - psz as i32) / 2,
                            psz as i32,
                            psz as i32,
                            buf,
                        );
                    }
                    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("icon.ico");
                    draw_text(
                        hdc,
                        name,
                        &RECT {
                            left: row.left + scale(40, dpi),
                            top: row.top,
                            right: del_rc.left - scale(8, dpi),
                            bottom: row.bottom,
                        },
                        th.text_primary,
                        fonts.body,
                        LEFT_VC,
                    );
                    subtle(
                        st,
                        hdc,
                        fonts,
                        del_rc,
                        Some(widgets::GLYPH_DELETE),
                        "",
                        Action::Ask(Confirm::DeleteIcon(p.clone())),
                        false,
                    );
                    cy += scale(40, dpi);
                }
            }
            // regions outside the visible (animating) part must not be clickable
            let vis = RECT {
                left: card_rc.left,
                top: ctop,
                right: card_rc.right,
                bottom: card_rc.bottom,
            };
            st.regions.retain_mut_from(regions_before, &vis);
            let _ = RestoreDC(hdc, saved);
        }
        y = card_rc.bottom + gap;
    }

    // 4. Show in sidebar
    {
        let cy = row_card(
            st,
            hdc,
            fonts,
            x,
            y,
            w,
            widgets::GLYPH_FOLDER_OPEN,
            tr("顯示在檔案總管側邊欄", "Show in the Explorer sidebar"),
            tr(
                "關閉時根資料夾仍可從「本機」或網址列開啟",
                "When off, the folders are still reachable from This PC or the address bar",
            ),
            scale(120, dpi),
        );
        toggle(st, hdc, fonts, x + w - pad, cy, ToggleId::ShowInSidebar);
        y += scale(68, dpi) + gap;
    }
    // 5. Hide from desktop
    {
        let cy = row_card(
            st,
            hdc,
            fonts,
            x,
            y,
            w,
            widgets::GLYPH_CANCEL,
            tr("不要顯示在桌面", "Hide from the desktop"),
            tr(
                "只出現在側邊欄，不在桌面上放圖示",
                "Show only in the sidebar, without a desktop icon",
            ),
            scale(120, dpi),
        );
        toggle(st, hdc, fonts, x + w - pad, cy, ToggleId::HideDesktop);
        y += scale(68, dpi) + gap;
    }

    // 6. Preview
    {
        let top = y;
        let n_subs = st
            .sub_indices()
            .iter()
            .filter(|&&i| st.config.subfolders[i].enabled)
            .count() as i32;
        let rows = 3 + if st.preview_expanded {
            n_subs.max(1)
        } else {
            0
        };
        let pv_h = rows * scale(30, dpi) + scale(16, dpi);
        let rc = rect(x, top, w, scale(68, dpi) + pv_h + pad);
        card_header(st, hdc, fonts, &rc, widgets::GLYPH_INFO, tr("側邊欄預覽", "Sidebar preview"), tr("按一下根資料夾可展開或摺疊；以目前設定繪製，尚未套用的變更也會顯示", "Click the root folder to expand or collapse; reflects current settings, including unapplied changes"));
        let pv = rect(
            inner_x,
            top + scale(68, dpi),
            inner_w.min(scale(360, dpi)),
            pv_h,
        );
        sidebar_preview(st, hdc, fonts, &pv);
        y = rc.bottom + gap;
    }

    // 7. Apply
    apply_card(st, hdc, fonts, x, y, w) + scale(0, dpi)
}

unsafe fn sidebar_preview(st: &mut AppState, hdc: HDC, fonts: &Fonts, pv: &RECT) {
    let dpi = st.dpi;
    let th = theme();
    widgets::sub_card(hdc, dpi, pv);
    let rh = scale(30, dpi);
    let mut ry = pv.top + scale(8, dpi);
    let row = |hdc: HDC, ry: i32, indent: i32, glyph: Option<&str>, text: &str, col: u32| {
        let gx = pv.left + scale(12 + 22, dpi) + indent;
        if let Some(g) = glyph {
            widgets::glyph(hdc, fonts.icon, g, &rect(gx, ry, scale(20, dpi), rh), col);
        }
        draw_text(
            hdc,
            text,
            &RECT {
                left: gx + scale(28, dpi),
                top: ry,
                right: pv.right - scale(8, dpi),
                bottom: ry + rh,
            },
            col,
            fonts.body,
            LEFT_VC,
        );
    };
    row(
        hdc,
        ry,
        0,
        Some("\u{E80F}"),
        tr("首頁", "Home"),
        th.text_muted,
    );
    ry += rh;
    // root row (clickable)
    let root_rc = RECT {
        left: pv.left + scale(4, dpi),
        top: ry,
        right: pv.right - scale(4, dpi),
        bottom: ry + rh,
    };
    let hot = st.interact(root_rc, Action::PreviewToggle, Cursor::Hand);
    if hot.hovered {
        round_box(
            hdc,
            &root_rc,
            scale(8, dpi),
            th.btn_hover_bg,
            th.btn_hover_bg,
        );
    }
    let chev = if st.preview_expanded {
        widgets::GLYPH_CHEVRON_DOWN
    } else {
        "\u{E76C}"
    };
    widgets::glyph(
        hdc,
        fonts.icon_small,
        chev,
        &rect(pv.left + scale(10, dpi), ry, scale(20, dpi), rh),
        th.text_secondary,
    );
    let root = st.root().clone();
    let isz = scale(16, dpi);
    let gx = pv.left + scale(12 + 22, dpi);
    if let Some(icon) = st.icons.get(&root.icon, isz) {
        let _ = DrawIconEx(
            hdc,
            gx + scale(2, dpi),
            ry + (rh - isz) / 2,
            icon,
            isz,
            isz,
            0,
            None,
            DI_NORMAL,
        );
    } else {
        widgets::glyph(
            hdc,
            fonts.icon,
            widgets::GLYPH_FOLDER,
            &rect(gx, ry, scale(20, dpi), rh),
            th.text_primary,
        );
    }
    let name = if root.name.trim().is_empty() {
        tr("（未命名）", "(unnamed)")
    } else {
        root.name.as_str()
    };
    draw_text(
        hdc,
        name,
        &RECT {
            left: gx + scale(28, dpi),
            top: ry,
            right: pv.right - scale(8, dpi),
            bottom: ry + rh,
        },
        th.text_bright,
        fonts.body,
        LEFT_VC,
    );
    ry += rh;
    if st.preview_expanded {
        let subs: Vec<(String, bool)> = st
            .sub_indices()
            .into_iter()
            .map(|i| &st.config.subfolders[i])
            .filter(|s| s.enabled)
            .map(|s| {
                (
                    s.name.clone(),
                    std::path::Path::new(&s.target_path).exists()
                        || s.target_path.starts_with("\\\\"),
                )
            })
            .collect();
        if subs.is_empty() {
            row(
                hdc,
                ry,
                scale(22, dpi),
                None,
                tr("（尚無啟用的子資料夾）", "(no enabled subfolders)"),
                th.text_dim,
            );
            ry += rh;
        }
        for (n, ok) in subs {
            row(
                hdc,
                ry,
                scale(22, dpi),
                Some(widgets::GLYPH_FOLDER),
                &n,
                if ok { th.text_primary } else { th.status_err },
            );
            ry += rh;
        }
    }
    row(
        hdc,
        ry,
        0,
        Some("\u{E7F8}"),
        tr("本機", "This PC"),
        th.text_muted,
    );
}

/// "Apply" card shared by the first two pages. Returns the y after it.
unsafe fn apply_card(st: &mut AppState, hdc: HDC, fonts: &Fonts, x: i32, y: i32, w: i32) -> i32 {
    let dpi = st.dpi;
    let th = theme();
    let pad = scale(16, dpi);
    let h = scale(68 + 32 + 16, dpi);
    let rc = rect(x, y, w, h);
    let dirty = st.is_dirty();
    let (status, col) = if !st.registered {
        (
            tr("尚未加入檔案總管", "Not added to Explorer yet"),
            th.status_warn,
        )
    } else if dirty {
        (
            tr("有尚未套用的變更", "You have unapplied changes"),
            th.status_warn,
        )
    } else {
        (
            tr(
                "已套用，側邊欄為最新狀態",
                "Applied; the sidebar is up to date",
            ),
            th.status_ok,
        )
    };
    card_header(st, hdc, fonts, &rc, widgets::GLYPH_REFRESH, tr("套用到檔案總管", "Apply to File Explorer"), tr("寫入目前使用者的登錄設定（不需要系統管理員權限）並通知檔案總管更新", "Writes per-user registry settings (no administrator rights needed) and tells Explorer to refresh"));
    let sw = text_width(hdc, status, fonts.small) + scale(24, dpi);
    widgets::status_dot(
        hdc,
        fonts,
        dpi,
        x + w - pad - sw,
        y + scale(24, dpi),
        col,
        status,
    );
    let by = y + scale(68, dpi);
    let ix = x + scale(56, dpi);
    let l1 = tr("套用", "Apply");
    let w1 = widgets::button_width(hdc, fonts, dpi, l1, false).max(scale(120, dpi));
    button(
        st,
        hdc,
        fonts,
        rect(ix, by, w1, scale(32, dpi)),
        l1,
        ButtonKind::Accent,
        Action::Apply,
        false,
    );
    let mut bx = ix + w1 + scale(8, dpi);
    let l2 = tr("重新啟動檔案總管", "Restart Explorer");
    let w2 = widgets::button_width(hdc, fonts, dpi, l2, false);
    button(
        st,
        hdc,
        fonts,
        rect(bx, by, w2, scale(32, dpi)),
        l2,
        ButtonKind::Standard,
        Action::Ask(Confirm::RestartExplorer),
        false,
    );
    bx += w2 + scale(8, dpi);
    let l3 = tr("從檔案總管移除", "Remove from Explorer");
    let w3 = widgets::button_width(hdc, fonts, dpi, l3, true);
    if bx + w3 <= x + w - pad {
        subtle(
            st,
            hdc,
            fonts,
            rect(bx, by, w3, scale(32, dpi)),
            Some(widgets::GLYPH_DELETE),
            l3,
            Action::Ask(Confirm::Unregister),
            !st.registered,
        );
    }
    rc.bottom + scale(14, dpi)
}

unsafe fn page_subfolders(
    st: &mut AppState,
    hdc: HDC,
    fonts: &Fonts,
    x: i32,
    mut y: i32,
    w: i32,
) -> i32 {
    let dpi = st.dpi;
    let th = theme();
    let gap = scale(14, dpi);
    let pad = scale(16, dpi);
    let inner_x = x + scale(56, dpi);
    let inner_w = w - scale(56, dpi) - pad;

    // 1. Root picker (only when there are several)
    if st.config.root_folders.len() > 1 {
        let top = y;
        let saved = SaveDC(hdc);
        let _ = IntersectClipRect(hdc, 0, 0, 0, 0);
        let before = st.regions.len();
        let end = root_selector(
            st,
            hdc,
            fonts,
            inner_x,
            top + scale(68, dpi),
            inner_w,
            false,
        );
        st.regions.truncate(before);
        let _ = RestoreDC(hdc, saved);
        let rc = RECT {
            left: x,
            top,
            right: x + w,
            bottom: end + pad,
        };
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_FOLDER,
            tr("根資料夾", "Root folder"),
            tr(
                "選擇要編輯哪一個根資料夾的子資料夾",
                "Choose which root folder's subfolders to edit",
            ),
        );
        root_selector(
            st,
            hdc,
            fonts,
            inner_x,
            top + scale(68, dpi),
            inner_w,
            false,
        );
        y = rc.bottom + gap;
    }

    // 2. Add / edit (expander)
    {
        let editing = st.editing_sub.is_some();
        let content_h = scale(16 + 20 + 34 + 12 + 20 + 34 + 16 + 32 + 16, dpi);
        let (title, desc, glyph) = if editing {
            (
                tr("編輯子資料夾", "Edit subfolder"),
                tr(
                    "修改名稱或對應的資料夾",
                    "Change the name or the mapped folder",
                ),
                widgets::GLYPH_EDIT,
            )
        } else {
            (
                tr("新增子資料夾", "Add a subfolder"),
                tr(
                    "把任何本機或網路資料夾放到根資料夾底下",
                    "Put any local or network folder under the root folder",
                ),
                widgets::GLYPH_NEW_FOLDER,
            )
        };
        let (card_rc, ctop, open) = render::expander(
            st,
            hdc,
            fonts,
            x,
            y,
            w,
            ExpanderId::SubEditor,
            glyph,
            title,
            desc,
            content_h,
        );
        if open > 0.0 {
            let saved = SaveDC(hdc);
            let _ = IntersectClipRect(
                hdc,
                card_rc.left,
                ctop + 1,
                card_rc.right,
                card_rc.bottom - 1,
            );
            let before = st.regions.len();
            let mut cy = ctop + scale(16, dpi);
            label(hdc, fonts, inner_x, cy, inner_w, dpi, tr("名稱", "Name"));
            cy += scale(20, dpi);
            textbox(
                st,
                hdc,
                fonts,
                rect(inner_x, cy, inner_w, scale(32, dpi)),
                Field::SubName,
                tr("例如：專案", "e.g. Projects"),
            );
            cy += scale(34 + 12, dpi);
            label(
                hdc,
                fonts,
                inner_x,
                cy,
                inner_w,
                dpi,
                tr("資料夾路徑", "Folder path"),
            );
            cy += scale(20, dpi);
            let bw = widgets::button_width(hdc, fonts, dpi, tr("瀏覽...", "Browse..."), false);
            textbox(
                st,
                hdc,
                fonts,
                rect(inner_x, cy, inner_w - bw - scale(8, dpi), scale(32, dpi)),
                Field::SubPath,
                tr(
                    "例如：D:\\Workspace 或 \\\\NAS\\Share",
                    "e.g. D:\\Workspace or \\\\NAS\\Share",
                ),
            );
            button(
                st,
                hdc,
                fonts,
                rect(inner_x + inner_w - bw, cy, bw, scale(32, dpi)),
                tr("瀏覽...", "Browse..."),
                ButtonKind::Standard,
                Action::BrowseSubPath,
                false,
            );
            cy += scale(34 + 16, dpi);
            let l1 = if editing {
                tr("儲存變更", "Save changes")
            } else {
                tr("新增", "Add")
            };
            let w1 = widgets::button_width(hdc, fonts, dpi, l1, false).max(scale(120, dpi));
            let can_save = !st.sub_name.trim().is_empty() && !st.sub_path.trim().is_empty();
            button(
                st,
                hdc,
                fonts,
                rect(inner_x, cy, w1, scale(32, dpi)),
                l1,
                ButtonKind::Accent,
                Action::SaveSub,
                !can_save,
            );
            if editing || !st.sub_name.is_empty() || !st.sub_path.is_empty() {
                let l2 = tr("取消", "Cancel");
                let w2 = widgets::button_width(hdc, fonts, dpi, l2, false);
                button(
                    st,
                    hdc,
                    fonts,
                    rect(inner_x + w1 + scale(8, dpi), cy, w2, scale(32, dpi)),
                    l2,
                    ButtonKind::Standard,
                    Action::CancelSub,
                    false,
                );
            }
            let vis = RECT {
                left: card_rc.left,
                top: ctop,
                right: card_rc.right,
                bottom: card_rc.bottom,
            };
            st.regions.retain_mut_from(before, &vis);
            let _ = RestoreDC(hdc, saved);
        }
        y = card_rc.bottom + gap;
    }

    // 3. List
    {
        let idxs = st.sub_indices();
        let row_h = scale(60, dpi);
        let top = y;
        let list_h = if idxs.is_empty() {
            scale(56, dpi)
        } else {
            idxs.len() as i32 * row_h
        };
        let rc = rect(x, top, w, scale(68, dpi) + list_h + scale(8, dpi));
        let title = format!("{}（{}）", tr("子資料夾", "Subfolders"), idxs.len());
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_FOLDER_OPEN,
            &title,
            tr(
                "勾選的項目會出現在側邊欄；變更會立即生效",
                "Checked entries appear in the sidebar; changes take effect immediately",
            ),
        );
        let mut ry = top + scale(68, dpi);
        if idxs.is_empty() {
            draw_text(
                hdc,
                tr(
                    "還沒有子資料夾。用上方的「新增子資料夾」加入第一個。",
                    "No subfolders yet. Use \"Add a subfolder\" above to add the first one.",
                ),
                &rect(inner_x, ry, inner_w, scale(40, dpi)),
                th.text_muted,
                fonts.body,
                LEFT_VC,
            );
        }
        let n = idxs.len();
        for (pos, &i) in idxs.iter().enumerate() {
            let sub = st.config.subfolders[i].clone();
            if pos > 0 {
                gdi::hline(hdc, x + pad, x + w - pad, ry, th.divider);
            }
            let cy = ry + row_h / 2;
            // checkbox
            let cb = scale(18, dpi);
            let cb_rc = rect(x + scale(19, dpi), cy - cb / 2, cb, cb);
            let hot = st.interact(
                RECT {
                    left: cb_rc.left - scale(6, dpi),
                    top: ry + scale(2, dpi),
                    right: cb_rc.right + scale(6, dpi),
                    bottom: ry + row_h - scale(4, dpi),
                },
                Action::SubEnabled(sub.id.clone()),
                Cursor::Hand,
            );
            let prog = st
                .checks
                .get(&sub.id)
                .map(|t| cfd_ui::anim::ease_out_cubic(t.raw()))
                .unwrap_or(1.0);
            widgets::checkbox(
                hdc,
                cb_rc.left,
                cb_rc.top,
                cb,
                sub.enabled,
                hot.hovered,
                false,
                prog,
            );
            // text
            let exists = std::path::Path::new(&sub.target_path).exists()
                || sub.target_path.starts_with("\\\\");
            let actions_w = scale(5 * 36 + 8, dpi);
            let tx = inner_x;
            let tw = x + w - pad - actions_w - tx;
            let name_col = if sub.enabled {
                th.text_bright
            } else {
                th.text_disabled
            };
            draw_text(
                hdc,
                &sub.name,
                &rect(tx, ry + scale(10, dpi), tw, scale(22, dpi)),
                name_col,
                fonts.body,
                LEFT_VC,
            );
            let (ptxt, pcol) = if exists {
                (sub.target_path.clone(), th.text_muted)
            } else {
                (
                    format!(
                        "{}（{}）",
                        sub.target_path,
                        tr("找不到這個路徑", "path not found")
                    ),
                    th.status_err,
                )
            };
            draw_text(
                hdc,
                &ptxt,
                &rect(tx, ry + scale(31, dpi), tw, scale(20, dpi)),
                pcol,
                fonts.small,
                LEFT_VC,
            );
            // command buttons
            let bs = scale(32, dpi);
            let mut bx = x + w - pad - bs;
            let by = cy - bs / 2;
            subtle(
                st,
                hdc,
                fonts,
                rect(bx, by, bs, bs),
                Some(widgets::GLYPH_DELETE),
                "",
                Action::Ask(Confirm::DeleteSub(i)),
                false,
            );
            bx -= bs + scale(4, dpi);
            subtle(
                st,
                hdc,
                fonts,
                rect(bx, by, bs, bs),
                Some(widgets::GLYPH_EDIT),
                "",
                Action::EditSub(i),
                false,
            );
            bx -= bs + scale(4, dpi);
            subtle(
                st,
                hdc,
                fonts,
                rect(bx, by, bs, bs),
                Some(widgets::GLYPH_OPEN_EXTERNAL),
                "",
                Action::OpenSub(i),
                !exists,
            );
            bx -= scale(6, dpi);
            gdi::vline(
                hdc,
                bx,
                by + scale(6, dpi),
                by + bs - scale(6, dpi),
                th.divider,
            );
            bx -= bs + scale(6, dpi);
            subtle(
                st,
                hdc,
                fonts,
                rect(bx, by, bs, bs),
                Some(widgets::GLYPH_DOWN),
                "",
                Action::SubDown(i),
                pos + 1 == n,
            );
            bx -= bs + scale(4, dpi);
            subtle(
                st,
                hdc,
                fonts,
                rect(bx, by, bs, bs),
                Some(widgets::GLYPH_UP),
                "",
                Action::SubUp(i),
                pos == 0,
            );
            ry += row_h;
        }
        y = rc.bottom + gap;
    }
    apply_card(st, hdc, fonts, x, y, w)
}

unsafe fn page_about(
    st: &mut AppState,
    hdc: HDC,
    fonts: &Fonts,
    x: i32,
    mut y: i32,
    w: i32,
) -> i32 {
    let dpi = st.dpi;
    let th = theme();
    let gap = scale(14, dpi);
    let pad = scale(16, dpi);
    let inner_x = x + scale(56, dpi);
    let inner_w = w - scale(56, dpi) - pad;

    // 1. Product
    {
        let desc = tr(
            "在 Windows 11 檔案總管左側邊欄加入自訂資料夾，並把任意本機或網路路徑整理在底下。只寫入目前使用者的設定，不需要系統管理員權限。",
            "Adds custom folders to the Windows 11 File Explorer sidebar and gathers any local or network paths under them. Only per-user settings are written; no administrator rights are needed.",
        );
        let dh = text_height_wrapped(hdc, desc, fonts.body, w - scale(96 + 16, dpi));
        let h = scale(20 + 26 + 22 + 8, dpi) + dh + scale(12 + 22 + 12 + 32 + 20, dpi);
        let rc = rect(x, y, w, h);
        widgets::card(hdc, dpi, &rc);
        let isz = scale(48, dpi);
        let exe = std::env::current_exe()
            .map(|p| format!("{},0", p.display()))
            .unwrap_or_default();
        let ix = x + scale(24, dpi);
        let iy = y + scale(20, dpi);
        match st.icons.get(&exe, isz) {
            Some(icon) => {
                let _ = DrawIconEx(hdc, ix, iy, icon, isz, isz, 0, None, DI_NORMAL);
            }
            None => {
                draw_aa_pill(
                    hdc,
                    ix,
                    iy,
                    isz,
                    isz,
                    accent_rgb(),
                    1.0,
                    accent_rgb(),
                    0.0,
                    0.0,
                );
                widgets::glyph(
                    hdc,
                    fonts.icon,
                    widgets::GLYPH_FOLDER,
                    &rect(ix, iy, isz, isz),
                    cfd_ui::theme::accent_contrast_text_bgr(),
                );
            }
        }
        let tx = x + scale(96, dpi);
        let tw = w - scale(96 + 16, dpi);
        let mut ty = y + scale(20, dpi);
        draw_text(
            hdc,
            "CustomerFD",
            &rect(tx, ty, tw, scale(26, dpi)),
            th.text_bright,
            fonts.title,
            LEFT_VC,
        );
        ty += scale(26, dpi);
        let ver = format!(
            "{} {}  ·  Windows 11 / 10",
            tr("版本", "Version"),
            env!("CARGO_PKG_VERSION")
        );
        draw_text(
            hdc,
            &ver,
            &rect(tx, ty, tw, scale(22, dpi)),
            th.text_secondary,
            fonts.small,
            LEFT_VC,
        );
        ty += scale(22 + 8, dpi);
        draw_text(
            hdc,
            desc,
            &rect(tx, ty, tw, dh + scale(4, dpi)),
            th.text_primary,
            fonts.body,
            WRAP,
        );
        ty += dh + scale(12, dpi);
        let cr = tr(
            "Copyright © 2026 ppundsh · 以 GNU GPL v3.0 或更新版本授權釋出 · 介面設計沿用 cbxshell-rs",
            "Copyright © 2026 ppundsh · Licensed under the GNU GPL v3.0 or later · UI design follows cbxshell-rs",
        );
        draw_text(
            hdc,
            cr,
            &rect(tx, ty, tw, scale(22, dpi)),
            th.text_muted,
            fonts.small,
            LEFT_VC,
        );
        ty += scale(22 + 12, dpi);
        let l1 = "GitHub";
        let w1 = widgets::button_width(hdc, fonts, dpi, l1, true);
        button_glyph(
            st,
            hdc,
            fonts,
            rect(tx, ty, w1, scale(32, dpi)),
            widgets::GLYPH_OPEN_EXTERNAL,
            l1,
            ButtonKind::Standard,
            Action::OpenGitHub,
            false,
        );
        let l2 = tr("授權條款", "License");
        let w2 = widgets::button_width(hdc, fonts, dpi, l2, false);
        subtle(
            st,
            hdc,
            fonts,
            rect(tx + w1 + scale(8, dpi), ty, w2, scale(32, dpi)),
            None,
            l2,
            Action::OpenLicense,
            false,
        );
        y = rc.bottom + gap;
    }

    // 2. Software update
    {
        let phase = updater::phase();
        let configured = updater::is_configured();
        let progress_h = if matches!(phase, Phase::Downloading { .. }) {
            scale(20, dpi)
        } else {
            0
        };
        let h = scale(68, dpi) + scale(32 + 16, dpi) + progress_h + scale(2, dpi) + scale(68, dpi);
        let rc = rect(x, y, w, h);
        let last = if st.config.last_update_check == 0 {
            tr("從未檢查", "never").to_string()
        } else {
            let (yy, mm, dd) = crate::update_core::unix_to_ymd(st.config.last_update_check);
            format!("{yy}-{mm:02}-{dd:02}")
        };
        let sub = format!(
            "{} {}  ·  {} {}",
            tr("目前版本", "Current version"),
            env!("CARGO_PKG_VERSION"),
            tr("上次檢查", "Last checked"),
            last
        );
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_DOWNLOAD,
            tr("軟體更新", "Software update"),
            &sub,
        );
        let (status, scol): (String, u32) = if !configured {
            (
                tr(
                    "此版本未設定更新簽章金鑰，請到 GitHub 下載新版",
                    "This build has no update signing key; download new versions from GitHub",
                )
                .into(),
                th.text_muted,
            )
        } else {
            match &phase {
                Phase::Idle => (String::new(), th.text_muted),
                Phase::Checking => (tr("正在檢查...", "Checking...").into(), th.text_muted),
                Phase::UpToDate => (tr("已是最新版本", "You're up to date").into(), th.status_ok),
                Phase::Available(m) => (
                    format!(
                        "{} {}",
                        tr("有新版本：", "New version available:"),
                        m.version
                    ),
                    accent_text_bgr(),
                ),
                Phase::Downloading {
                    done,
                    total,
                    version,
                } => {
                    let pct = if *total > 0 { done * 100 / total } else { 0 };
                    (
                        format!(
                            "{} {version}  {pct}%",
                            tr("正在下載並驗證", "Downloading and verifying")
                        ),
                        th.text_secondary,
                    )
                }
                Phase::Launching => (
                    tr("正在啟動安裝程式...", "Starting the installer...").into(),
                    th.text_secondary,
                ),
                Phase::Error(e) => (e.clone(), th.status_err),
            }
        };
        if !status.is_empty() {
            let sw = text_width(hdc, &status, fonts.small).min(w / 2);
            draw_text(
                hdc,
                &status,
                &rect(x + w - pad - sw, y + scale(14, dpi), sw, scale(22, dpi)),
                scol,
                fonts.small,
                LEFT_VC,
            );
        }
        let by = y + scale(68, dpi);
        let busy = updater::is_busy();
        let l1 = tr("檢查更新", "Check for updates");
        let w1 = widgets::button_width(hdc, fonts, dpi, l1, true);
        button_glyph(
            st,
            hdc,
            fonts,
            rect(inner_x, by, w1, scale(32, dpi)),
            widgets::GLYPH_REFRESH,
            l1,
            ButtonKind::Standard,
            Action::CheckUpdate,
            busy || !configured,
        );
        let mut bx = inner_x + w1 + scale(8, dpi);
        let available = matches!(phase, Phase::Available(_));
        let l2 = tr("下載並安裝", "Download and install");
        let w2 = widgets::button_width(hdc, fonts, dpi, l2, true);
        button_glyph(
            st,
            hdc,
            fonts,
            rect(bx, by, w2, scale(32, dpi)),
            widgets::GLYPH_DOWNLOAD,
            l2,
            ButtonKind::Accent,
            Action::Ask(Confirm::InstallUpdate),
            !available || busy,
        );
        bx += w2 + scale(8, dpi);
        let l3 = tr("版本資訊", "Release notes");
        let w3 = widgets::button_width(hdc, fonts, dpi, l3, false);
        if bx + w3 <= x + w - pad {
            subtle(
                st,
                hdc,
                fonts,
                rect(bx, by, w3, scale(32, dpi)),
                None,
                l3,
                Action::ReleaseNotes,
                false,
            );
        }
        let mut cy = by + scale(32 + 16, dpi);
        if let Phase::Downloading { done, total, .. } = phase {
            let pw = inner_w;
            let ph = scale(4, dpi);
            draw_aa_pill(
                hdc,
                inner_x,
                cy,
                pw,
                ph,
                bgr_to_rgb(th.divider),
                1.0,
                0,
                0.0,
                0.0,
            );
            let frac = if total > 0 {
                done as f32 / total as f32
            } else {
                0.0
            };
            let fw = ((pw as f32) * frac.clamp(0.0, 1.0)).round() as i32;
            if fw >= ph {
                draw_aa_pill(hdc, inner_x, cy, fw, ph, accent_rgb(), 1.0, 0, 0.0, 0.0);
            }
            cy += progress_h;
        }
        widgets::expander_seam(hdc, dpi, x, cy, w);
        cy += scale(2, dpi);
        let row_mid = cy + scale(34, dpi);
        draw_text(
            hdc,
            tr("自動檢查更新", "Check for updates automatically"),
            &rect(
                inner_x,
                row_mid - scale(20, dpi),
                inner_w - scale(120, dpi),
                scale(22, dpi),
            ),
            th.text_primary,
            fonts.body,
            LEFT_VC,
        );
        draw_text(
            hdc,
            tr(
                "每天最多檢查一次，只會提示，不會自動下載或安裝",
                "At most once a day; it only tells you, never downloads or installs by itself",
            ),
            &rect(
                inner_x,
                row_mid + scale(1, dpi),
                inner_w - scale(120, dpi),
                scale(20, dpi),
            ),
            th.text_muted,
            fonts.small,
            LEFT_VC,
        );
        toggle(st, hdc, fonts, x + w - pad, row_mid, ToggleId::AutoUpdate);
        y = rc.bottom + gap;
    }

    // 3. Install info
    {
        let dll = registered_dll_path()
            .or_else(|| find_shell_dll().map(|p| p.display().to_string()))
            .unwrap_or_else(|| tr("找不到 cfd_shell.dll", "cfd_shell.dll not found").to_string());
        let cfg = config_dir().display().to_string();
        let lines = [
            (tr("擴充套件", "Extension"), dll),
            (tr("設定資料夾", "Settings folder"), cfg),
            (
                tr("CLSID 範圍", "CLSID range"),
                "{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042} … 1081".to_string(),
            ),
            (
                tr("更新驗證", "Update check"),
                if !updater::is_configured() {
                    tr(
                        "未設定簽章金鑰（停用自動更新）",
                        "No signing key (automatic updates disabled)",
                    )
                    .to_string()
                } else if updater::requires_approval() {
                    tr(
                        "minisign 簽章＋維護者 YubiKey 核准",
                        "minisign signature + maintainer YubiKey approval",
                    )
                    .to_string()
                } else {
                    tr(
                        "minisign（Ed25519）數位簽章",
                        "minisign (Ed25519) digital signature",
                    )
                    .to_string()
                },
            ),
        ];
        let h = scale(68, dpi) + lines.len() as i32 * scale(26, dpi) + scale(12 + 32 + 16, dpi);
        let rc = rect(x, y, w, h);
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_SETTINGS,
            tr("安裝資訊", "Installation"),
            tr(
                "檔案位置與登錄範圍，回報問題時很有用",
                "File locations and registry range — useful when reporting a problem",
            ),
        );
        let mut ly = y + scale(68, dpi);
        let kw = scale(110, dpi);
        for (k, v) in lines {
            draw_text(
                hdc,
                k,
                &rect(inner_x, ly, kw, scale(24, dpi)),
                th.text_muted,
                fonts.small,
                LEFT_VC,
            );
            draw_text(
                hdc,
                &v,
                &rect(inner_x + kw, ly, inner_w - kw, scale(24, dpi)),
                th.text_primary,
                fonts.small,
                render::LEFT_VC,
            );
            ly += scale(26, dpi);
        }
        ly += scale(12, dpi);
        let l1 = tr("開啟設定資料夾", "Open settings folder");
        let w1 = widgets::button_width(hdc, fonts, dpi, l1, false);
        button(
            st,
            hdc,
            fonts,
            rect(inner_x, ly, w1, scale(32, dpi)),
            l1,
            ButtonKind::Standard,
            Action::OpenConfigDir,
            false,
        );
        let l2 = tr("開啟安裝資料夾", "Open install folder");
        let w2 = widgets::button_width(hdc, fonts, dpi, l2, false);
        button(
            st,
            hdc,
            fonts,
            rect(inner_x + w1 + scale(8, dpi), ly, w2, scale(32, dpi)),
            l2,
            ButtonKind::Standard,
            Action::OpenInstallDir,
            false,
        );
        y = rc.bottom + gap;
    }

    // 4. Cleanup
    {
        let warn = tr(
            "移除所有登錄項目、設定檔與自訂圖示。要解除安裝程式本身，請到「設定 › 應用程式 › 已安裝的應用程式」。",
            "Removes every registry entry, the settings file and all custom icons. To uninstall the program itself, use Settings › Apps › Installed apps.",
        );
        let wh = text_height_wrapped(hdc, warn, fonts.body, inner_w - scale(44 + 16, dpi));
        let banner_h = wh + scale(24, dpi);
        let h = scale(68, dpi) + banner_h + scale(12 + 32 + 16, dpi);
        let rc = rect(x, y, w, h);
        card_header(
            st,
            hdc,
            fonts,
            &rc,
            widgets::GLYPH_DELETE,
            tr("移除與清理", "Remove and clean up"),
            tr(
                "讓電腦回到沒有安裝過 CustomerFD 設定的狀態",
                "Return the PC to a state without any CustomerFD settings",
            ),
        );
        let banner = rect(inner_x, y + scale(68, dpi), inner_w, banner_h);
        round_box(
            hdc,
            &banner,
            scale(8, dpi),
            th.caution_bg,
            th.caution_border,
        );
        widgets::glyph(
            hdc,
            fonts.icon,
            widgets::GLYPH_WARNING,
            &rect(
                banner.left + scale(12, dpi),
                banner.top + scale(10, dpi),
                scale(20, dpi),
                scale(20, dpi),
            ),
            th.status_warn,
        );
        draw_text(
            hdc,
            warn,
            &RECT {
                left: banner.left + scale(44, dpi),
                top: banner.top + scale(12, dpi),
                right: banner.right - scale(16, dpi),
                bottom: banner.bottom,
            },
            if th.dark {
                th.caution_text
            } else {
                th.text_primary
            },
            fonts.body,
            WRAP,
        );
        let l = tr("全部清理...", "Clean up everything...");
        let bw = widgets::button_width(hdc, fonts, dpi, l, false);
        button(
            st,
            hdc,
            fonts,
            rect(inner_x, banner.bottom + scale(12, dpi), bw, scale(32, dpi)),
            l,
            ButtonKind::Standard,
            Action::Ask(Confirm::Cleanup),
            false,
        );
        y = rc.bottom + gap;
    }
    let _ = (CENTER_VC, Severity::Info);
    y
}

/// Region helpers that need to know the visible part of an animating container.
trait RegionClip {
    /// Drops or clips regions pushed since `from` to `vis`.
    fn retain_mut_from(&mut self, from: usize, vis: &RECT);
}

impl RegionClip for Vec<crate::app::Region> {
    fn retain_mut_from(&mut self, from: usize, vis: &RECT) {
        let tail: Vec<crate::app::Region> = self.drain(from..).collect();
        for mut r in tail {
            r.rect.top = r.rect.top.max(vis.top);
            r.rect.bottom = r.rect.bottom.min(vis.bottom);
            if r.rect.bottom > r.rect.top {
                self.push(r);
            }
        }
    }
}
