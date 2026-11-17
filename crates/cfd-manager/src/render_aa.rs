use std::ffi::c_void;
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC,
};

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

#[inline]
pub fn lerp_color_rgb(c1: u32, c2: u32, t: f32) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let r1 = ((c1 >> 16) & 0xFF) as f32;
    let g1 = ((c1 >> 8) & 0xFF) as f32;
    let b1 = (c1 & 0xFF) as f32;

    let r2 = ((c2 >> 16) & 0xFF) as f32;
    let g2 = ((c2 >> 8) & 0xFF) as f32;
    let b2 = (c2 & 0xFF) as f32;

    let r = (r1 + (r2 - r1) * t).round().clamp(0.0, 255.0) as u32;
    let g = (g1 + (g2 - g1) * t).round().clamp(0.0, 255.0) as u32;
    let b = (b1 + (b2 - b1) * t).round().clamp(0.0, 255.0) as u32;
    (r << 16) | (g << 8) | b
}

unsafe fn create_hbitmap_from_bgra(w: u32, h: u32, bgra: &[u8]) -> Option<HBITMAP> {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let dc = CreateCompatibleDC(HDC::default());
    let hbmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok();
    let _ = DeleteDC(dc);
    let hbmp = hbmp?;
    if !bits.is_null() {
        let len = (w * h * 4) as usize;
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), len.min(bgra.len()));
    }
    Some(hbmp)
}

pub unsafe fn blend_bgra_to_hdc(hdc: HDC, x: i32, y: i32, w: i32, h: i32, bgra: &[u8]) {
    if w <= 0 || h <= 0 || bgra.len() < (w * h * 4) as usize {
        return;
    }
    let Some(hbmp) = create_hbitmap_from_bgra(w as u32, h as u32, bgra) else {
        return;
    };
    let mem_dc = CreateCompatibleDC(hdc);
    let old_bmp = SelectObject(mem_dc, hbmp);
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let _ = AlphaBlend(hdc, x, y, w, h, mem_dc, 0, 0, w, h, blend);
    SelectObject(mem_dc, old_bmp);
    let _ = DeleteDC(mem_dc);
    let _ = DeleteObject(hbmp);
}

pub unsafe fn draw_aa_pill(
    hdc: HDC,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    fill_rgb: u32,
    fill_alpha: f32,
    border_rgb: u32,
    border_alpha: f32,
    border_width: f32,
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let r = (h as f32) / 2.0;
    let max_rx = (w as f32 - r).max(r);

    let (fr, fg, fb) = (
        ((fill_rgb >> 16) & 0xFF) as f32,
        ((fill_rgb >> 8) & 0xFF) as f32,
        (fill_rgb & 0xFF) as f32,
    );
    let (br, bg, bb) = (
        ((border_rgb >> 16) & 0xFF) as f32,
        ((border_rgb >> 8) & 0xFF) as f32,
        (border_rgb & 0xFF) as f32,
    );

    for py in 0..h {
        let cy = py as f32 + 0.5;
        let dy = cy - r;
        let dy2 = dy * dy;
        for px in 0..w {
            let cx = px as f32 + 0.5;
            let clamped_x = cx.clamp(r, max_rx);
            let dx = cx - clamped_x;
            let dist = (dx * dx + dy2).sqrt();
            let signed_dist = dist - r;
            let outer_cov = (0.5 - signed_dist).clamp(0.0, 1.0);
            if outer_cov <= 0.0 {
                continue;
            }
            let (cr, cg, cb, ca) = if border_width > 0.0 {
                let border_edge = signed_dist + border_width;
                if border_edge >= 0.0 {
                    let t = (border_edge / 0.75).clamp(0.0, 1.0);
                    (
                        fr + (br - fr) * t,
                        fg + (bg - fg) * t,
                        fb + (bb - fb) * t,
                        (fill_alpha + (border_alpha - fill_alpha) * t) * outer_cov,
                    )
                } else {
                    (fr, fg, fb, fill_alpha * outer_cov)
                }
            } else {
                (fr, fg, fb, fill_alpha * outer_cov)
            };

            let a = ca.clamp(0.0, 1.0);
            let idx = ((py * w + px) * 4) as usize;
            buf[idx] = (cb * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 1] = (cg * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 2] = (cr * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    blend_bgra_to_hdc(hdc, x, y, w, h, &buf);
}

pub unsafe fn draw_aa_circle(
    hdc: HDC,
    center_x: i32,
    center_y: i32,
    radius: f32,
    fill_rgb: u32,
    fill_alpha: f32,
    border_rgb: u32,
    border_alpha: f32,
    border_width: f32,
) {
    if radius <= 0.0 {
        return;
    }
    let side = ((radius + border_width) * 2.0).ceil() as i32 + 3;
    let mut buf = vec![0u8; (side * side * 4) as usize];
    let center = side as f32 / 2.0;

    let (fr, fg, fb) = (
        ((fill_rgb >> 16) & 0xFF) as f32,
        ((fill_rgb >> 8) & 0xFF) as f32,
        (fill_rgb & 0xFF) as f32,
    );
    let (br, bg, bb) = (
        ((border_rgb >> 16) & 0xFF) as f32,
        ((border_rgb >> 8) & 0xFF) as f32,
        (border_rgb & 0xFF) as f32,
    );

    for py in 0..side {
        let dy = (py as f32 + 0.5) - center;
        let dy2 = dy * dy;
        for px in 0..side {
            let dx = (px as f32 + 0.5) - center;
            let dist = (dx * dx + dy2).sqrt();
            let signed_dist = dist - radius;
            let outer_cov = (0.5 - signed_dist).clamp(0.0, 1.0);
            if outer_cov <= 0.0 {
                continue;
            }
            let (cr, cg, cb, ca) = if border_width > 0.0 {
                let border_edge = signed_dist + border_width;
                if border_edge >= 0.0 {
                    let t = (border_edge / 0.75).clamp(0.0, 1.0);
                    (
                        fr + (br - fr) * t,
                        fg + (bg - fg) * t,
                        fb + (bb - fb) * t,
                        (fill_alpha + (border_alpha - fill_alpha) * t) * outer_cov,
                    )
                } else {
                    (fr, fg, fb, fill_alpha * outer_cov)
                }
            } else {
                (fr, fg, fb, fill_alpha * outer_cov)
            };

            let a = ca.clamp(0.0, 1.0);
            let idx = ((py * side + px) * 4) as usize;
            buf[idx] = (cb * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 1] = (cg * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 2] = (cr * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    let x = center_x - (side / 2);
    let y = center_y - (side / 2);
    blend_bgra_to_hdc(hdc, x, y, side, side, &buf);
}

#[inline]
fn dist_to_segment(px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let vx = x2 - x1;
    let vy = y2 - y1;
    let wx = px - x1;
    let wy = py - y1;
    let len2 = vx * vx + vy * vy;
    if len2 <= 1e-6 {
        return (wx * wx + wy * wy).sqrt();
    }
    let t = ((wx * vx + wy * vy) / len2).clamp(0.0, 1.0);
    let dx = px - (x1 + t * vx);
    let dy = py - (y1 + t * vy);
    (dx * dx + dy * dy).sqrt()
}

/// Renders a crisp anti-aliased chevron (`v` when expanded, `>` when collapsed).
pub unsafe fn draw_aa_chevron(
    hdc: HDC,
    cx: i32,
    cy: i32,
    half_size: f32,
    expanded: bool,
    rgb: u32,
    alpha: f32,
    stroke_w: f32,
) {
    let side = (half_size * 2.0 + stroke_w * 2.0).ceil() as i32 + 4;
    let mut buf = vec![0u8; (side * side * 4) as usize];
    let mid = side as f32 / 2.0;

    let (p0, p1, p2) = if expanded {
        (
            (mid - half_size, mid - half_size * 0.45),
            (mid, mid + half_size * 0.55),
            (mid + half_size, mid - half_size * 0.45),
        )
    } else {
        (
            (mid - half_size * 0.45, mid - half_size),
            (mid + half_size * 0.55, mid),
            (mid - half_size * 0.45, mid + half_size),
        )
    };

    let r = ((rgb >> 16) & 0xFF) as f32;
    let g = ((rgb >> 8) & 0xFF) as f32;
    let b = (rgb & 0xFF) as f32;
    let half_stroke = stroke_w * 0.5;

    for py in 0..side {
        let fy = py as f32 + 0.5;
        for px in 0..side {
            let fx = px as f32 + 0.5;
            let d1 = dist_to_segment(fx, fy, p0.0, p0.1, p1.0, p1.1);
            let d2 = dist_to_segment(fx, fy, p1.0, p1.1, p2.0, p2.1);
            let d = d1.min(d2);
            let cov = (0.5 + half_stroke - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let a = (alpha * cov).clamp(0.0, 1.0);
            let idx = ((py * side + px) * 4) as usize;
            buf[idx] = (b * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 1] = (g * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 2] = (r * a).round().clamp(0.0, 255.0) as u8;
            buf[idx + 3] = (a * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    blend_bgra_to_hdc(hdc, cx - side / 2, cy - side / 2, side, side, &buf);
}

/// Fills the right content panel (`sidebar_w..buf_w, 0..buf_h`) with an anti-aliased
/// rounded top-left corner and subtle 1px border, matching Win11 Task Manager / cbxshell-rs.
pub unsafe fn fill_content_panel_aa(
    bits: *mut u32,
    buf_w: i32,
    buf_h: i32,
    sidebar_w: i32,
    radius: i32,
    bg_bgr: u32,
    border_bgr: u32,
) {
    if bits.is_null() || buf_w <= sidebar_w || buf_h <= 0 {
        return;
    }
    let bg_rgb = bgr_to_rgb(bg_bgr);
    let bd_rgb = bgr_to_rgb(border_bgr);
    let (br, bg, bb) = (
        ((bg_rgb >> 16) & 0xFF) as f32,
        ((bg_rgb >> 8) & 0xFF) as f32,
        (bg_rgb & 0xFF) as f32,
    );
    let (dr, dg, db) = (
        ((bd_rgb >> 16) & 0xFF) as f32,
        ((bd_rgb >> 8) & 0xFF) as f32,
        (bd_rgb & 0xFF) as f32,
    );
    let opaque_bg = 0xFF00_0000 | ((bg_rgb & 0xFF) << 0) | (bg_rgb & 0xFF00) | (bg_rgb & 0xFF_0000);
    let opaque_bd = 0xFF00_0000 | bd_rgb;
    let r = radius.max(1) as f32;
    let corner_x = (sidebar_w + radius).min(buf_w);
    let corner_y = radius.min(buf_h);

    for py in 0..buf_h {
        let row = bits.add((py * buf_w) as usize);
        if py < corner_y {
            let cy = py as f32 + 0.5;
            let dy = r - cy;
            for px in sidebar_w..corner_x {
                let cx = (px - sidebar_w) as f32 + 0.5;
                let dx = r - cx;
                let dist = (dx * dx + dy * dy).sqrt();
                let signed = dist - r;
                let outer_cov = (0.5 - signed).clamp(0.0, 1.0);
                if outer_cov <= 0.0 {
                    continue;
                }
                let border_t = (signed + 1.1).clamp(0.0, 1.0);
                let cr = br + (dr - br) * border_t;
                let cg = bg + (dg - bg) * border_t;
                let cb = bb + (db - bb) * border_t;
                let a = outer_cov;
                let pr = (cr * a).round().clamp(0.0, 255.0) as u32;
                let pg = (cg * a).round().clamp(0.0, 255.0) as u32;
                let pb = (cb * a).round().clamp(0.0, 255.0) as u32;
                let pa = (a * 255.0).round().clamp(0.0, 255.0) as u32;
                *row.add(px as usize) = (pa << 24) | (pr << 16) | (pg << 8) | pb;
            }
            // Top 1px horizontal border after corner
            for px in corner_x..buf_w {
                *row.add(px as usize) = if py == 0 { opaque_bd } else { opaque_bg };
            }
        } else {
            // Left 1px vertical border at sidebar_w, opaque bg after
            *row.add(sidebar_w as usize) = opaque_bd;
            for px in (sidebar_w + 1)..buf_w {
                *row.add(px as usize) = opaque_bg;
            }
        }
    }
}

/// Stamps `0xFF` alpha onto the content panel while preserving the anti-aliased top-left corner.
pub unsafe fn stamp_content_panel_alpha(
    bits: *mut u32,
    buf_w: i32,
    buf_h: i32,
    sidebar_w: i32,
    radius: i32,
) {
    if bits.is_null() || buf_w <= sidebar_w || buf_h <= 0 {
        return;
    }
    let r = radius.max(1) as f32;
    let corner_x = (sidebar_w + radius).min(buf_w);
    let corner_y = radius.min(buf_h);

    for py in 0..buf_h {
        let row = bits.add((py * buf_w) as usize);
        let start_x = if py < corner_y { corner_x } else { sidebar_w };
        for px in start_x..buf_w {
            *row.add(px as usize) |= 0xFF00_0000;
        }
        if py < corner_y {
            let cy = py as f32 + 0.5;
            let dy = r - cy;
            for px in sidebar_w..corner_x {
                let cx = (px - sidebar_w) as f32 + 0.5;
                let dx = r - cx;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist <= r - 0.5 {
                    *row.add(px as usize) |= 0xFF00_0000;
                }
            }
        }
    }
}
