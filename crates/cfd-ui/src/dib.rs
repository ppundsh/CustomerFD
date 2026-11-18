//! 32-bit premultiplied BGRA back buffer used for the translucent (Mica) parts of a window.
//!
//! Ported from cbxshell-rs. GDI writes alpha 0 into every pixel it touches, so the rail is drawn
//! only through the premultiplied helpers here and `DrawThemeTextEx(DTT_COMPOSITED)`, and the
//! opaque content panel is re-stamped to alpha 255 after GDI drawing.

#![allow(clippy::too_many_arguments)]

use windows::Win32::Foundation::RECT;

fn box_blur_1d(
    src: &[[u32; 4]],
    dst: &mut [[u32; 4]],
    w: usize,
    h: usize,
    r: usize,
    horizontal: bool,
) {
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let idx = |o: usize, i: usize| if horizontal { o * w + i } else { i * w + o };
    let win = (2 * r + 1) as u32;
    for o in 0..outer {
        let mut sum = [0u32; 4];
        for k in 0..=(2 * r) {
            let i = k.saturating_sub(r).min(inner - 1);
            let p = src[idx(o, i)];
            for c in 0..4 {
                sum[c] += p[c];
            }
        }
        for i in 0..inner {
            let mut out = [0u32; 4];
            for c in 0..4 {
                out[c] = sum[c] / win;
            }
            dst[idx(o, i)] = out;
            let add = src[idx(o, (i + r + 1).min(inner - 1))];
            let sub = src[idx(o, i.saturating_sub(r))];
            for c in 0..4 {
                sum[c] = sum[c] + add[c] - sub[c];
            }
        }
    }
}

/// Anti-aliased coverage (0..1) of a pixel centre inside a rounded rectangle.
fn round_rect_coverage(fx: f32, fy: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> f32 {
    let cx = fx.clamp(x0 + r, x1 - r);
    let cy = fy.clamp(y0 + r, y1 - r);
    let d = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() - r;
    let inside_box = fx >= x0 && fx <= x1 && fy >= y0 && fy <= y1;
    if !inside_box {
        return 0.0;
    }
    (0.5 - d).clamp(0.0, 1.0)
}

pub struct DibSurface {
    pub bits: *mut u32,
    pub w: i32,
    pub h: i32,
}

impl DibSurface {
    #[inline]
    pub unsafe fn blend_premul(&self, x: i32, y: i32, b: u8, g: u8, r: u8, a: u8) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || a == 0 {
            return;
        }
        let idx = (y as usize) * (self.w as usize) + (x as usize);
        let dst = *self.bits.add(idx);
        let da = (dst >> 24) & 0xFF;
        let dr = (dst >> 16) & 0xFF;
        let dg = (dst >> 8) & 0xFF;
        let db = dst & 0xFF;

        let sa = a as u32;
        let sr = (r as u32 * sa + 127) / 255;
        let sg = (g as u32 * sa + 127) / 255;
        let sb = (b as u32 * sa + 127) / 255;
        let inv = 255 - sa;

        let oa = (sa + (da * inv + 127) / 255).min(255);
        let or = (sr + (dr * inv + 127) / 255).min(255);
        let og = (sg + (dg * inv + 127) / 255).min(255);
        let ob = (sb + (db * inv + 127) / 255).min(255);
        *self.bits.add(idx) = (oa << 24) | (or << 16) | (og << 8) | ob;
    }

    pub unsafe fn fill_rounded_translucent(
        &self,
        x0: i32,
        y0: i32,
        w: i32,
        h: i32,
        radius: i32,
        fill_bgr: u32,
        fill_a: u8,
        border_bgr: u32,
        border_a: u8,
    ) {
        if w <= 0 || h <= 0 {
            return;
        }
        let r = (radius as f32).min((w.min(h) as f32) * 0.5);
        let fb = (fill_bgr >> 16) as u8;
        let fg = ((fill_bgr >> 8) & 0xFF) as u8;
        let fr = (fill_bgr & 0xFF) as u8;
        let bb = (border_bgr >> 16) as u8;
        let bg = ((border_bgr >> 8) & 0xFF) as u8;
        let br = (border_bgr & 0xFF) as u8;

        let x1 = (x0 + w).min(self.w);
        let y1 = (y0 + h).min(self.h);
        let sx0 = x0.max(0);
        let sy0 = y0.max(0);
        let cx0 = x0 as f32 + r;
        let cx1 = (x0 + w) as f32 - r;
        let cy0 = y0 as f32 + r;
        let cy1 = (y0 + h) as f32 - r;

        for py in sy0..y1 {
            let fy = py as f32 + 0.5;
            let dy = if fy < cy0 {
                cy0 - fy
            } else if fy > cy1 {
                fy - cy1
            } else {
                0.0
            };
            for px in sx0..x1 {
                let fx = px as f32 + 0.5;
                let dx = if fx < cx0 {
                    cx0 - fx
                } else if fx > cx1 {
                    fx - cx1
                } else {
                    0.0
                };
                let dist = (dx * dx + dy * dy).sqrt() - r;
                if dist > 0.5 {
                    continue;
                }
                let cov = (0.5 - dist).clamp(0.0, 1.0);
                if dist > -1.0 && border_a > 0 {
                    let ba = ((border_a as f32) * cov).round() as u8;
                    self.blend_premul(px, py, bb, bg, br, ba);
                } else if fill_a > 0 {
                    let fa = ((fill_a as f32) * cov).round() as u8;
                    self.blend_premul(px, py, fb, fg, fr, fa);
                }
            }
        }
    }

    /// In-app acrylic: blurs the premultiplied pixels inside a rounded rectangle (three box-blur
    /// passes ≈ Gaussian), blending at the anti-aliased edge so corners stay round.
    pub unsafe fn blur_round_rect(
        &self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        radius: i32,
        blur: i32,
    ) {
        let sx0 = x0.max(0);
        let sy0 = y0.max(0);
        let sx1 = x1.min(self.w);
        let sy1 = y1.min(self.h);
        let (w, h) = (sx1 - sx0, sy1 - sy0);
        if w <= 2 || h <= 2 || blur <= 0 {
            return;
        }
        let (wu, hu) = (w as usize, h as usize);
        let mut buf: Vec<[u32; 4]> = Vec::with_capacity(wu * hu);
        for py in sy0..sy1 {
            let row = (py as usize) * (self.w as usize);
            for px in sx0..sx1 {
                let v = *self.bits.add(row + px as usize);
                buf.push([v & 0xFF, (v >> 8) & 0xFF, (v >> 16) & 0xFF, v >> 24]);
            }
        }
        let r = (blur / 3).max(1) as usize;
        let mut tmp = buf.clone();
        for _ in 0..3 {
            box_blur_1d(&buf, &mut tmp, wu, hu, r, true);
            box_blur_1d(&tmp, &mut buf, wu, hu, r, false);
        }
        let rad = (radius as f32).min(w.min(h) as f32 * 0.5);
        for py in sy0..sy1 {
            let row = (py as usize) * (self.w as usize);
            for px in sx0..sx1 {
                let c = round_rect_coverage(
                    px as f32 + 0.5,
                    py as f32 + 0.5,
                    x0 as f32,
                    y0 as f32,
                    x1 as f32,
                    y1 as f32,
                    rad,
                );
                if c <= 0.0 {
                    continue;
                }
                let b = buf[((py - sy0) as usize) * wu + (px - sx0) as usize];
                let ptr = self.bits.add(row + px as usize);
                let o = *ptr;
                let mix = |ov: u32, nv: u32| -> u32 {
                    ((ov as f32) * (1.0 - c) + (nv as f32) * c)
                        .round()
                        .clamp(0.0, 255.0) as u32
                };
                *ptr = mix(o & 0xFF, b[0])
                    | (mix((o >> 8) & 0xFF, b[1]) << 8)
                    | (mix((o >> 16) & 0xFF, b[2]) << 16)
                    | (mix(o >> 24, b[3]) << 24);
            }
        }
    }

    pub unsafe fn stamp_opaque_round_rect(&self, x0: i32, y0: i32, x1: i32, y1: i32, radius: i32) {
        let sx0 = x0.max(0);
        let sy0 = y0.max(0);
        let sx1 = x1.min(self.w);
        let sy1 = y1.min(self.h);
        let r = (radius as f32).min(((x1 - x0).min(y1 - y0) as f32) * 0.5);
        let cx0 = x0 as f32 + r;
        let cx1 = x1 as f32 - r;
        let cy0 = y0 as f32 + r;
        let cy1 = y1 as f32 - r;

        for py in sy0..sy1 {
            let fy = py as f32 + 0.5;
            let dy = if fy < cy0 {
                cy0 - fy
            } else if fy > cy1 {
                fy - cy1
            } else {
                0.0
            };
            let row_off = (py as usize) * (self.w as usize);
            for px in sx0..sx1 {
                let fx = px as f32 + 0.5;
                let dx = if fx < cx0 {
                    cx0 - fx
                } else if fx > cx1 {
                    fx - cx1
                } else {
                    0.0
                };
                if dx * dx + dy * dy <= (r + 0.5) * (r + 0.5) {
                    let ptr = self.bits.add(row_off + (px as usize));
                    *ptr |= 0xFF00_0000;
                }
            }
        }
    }

    pub unsafe fn fill_content_panel_aa(
        &self,
        sidebar_w: i32,
        radius: i32,
        bg_bgr: u32,
        border_bgr: u32,
    ) {
        let sx0 = sidebar_w.max(0);
        let r_f = radius as f32;
        let bg_b = (bg_bgr >> 16) as u8;
        let bg_g = ((bg_bgr >> 8) & 0xFF) as u8;
        let bg_r = (bg_bgr & 0xFF) as u8;
        let bd_b = (border_bgr >> 16) as u8;
        let bd_g = ((border_bgr >> 8) & 0xFF) as u8;
        let bd_r = (border_bgr & 0xFF) as u8;
        let bg_pixel = 0xFF00_0000 | ((bg_r as u32) << 16) | ((bg_g as u32) << 8) | (bg_b as u32);
        let bd_pixel = 0xFF00_0000 | ((bd_r as u32) << 16) | ((bd_g as u32) << 8) | (bd_b as u32);

        for py in 0..self.h {
            let row_off = (py as usize) * (self.w as usize);
            let in_top_arc_y = py < radius;
            for px in sx0..self.w {
                let ptr = self.bits.add(row_off + (px as usize));
                if in_top_arc_y && px < sidebar_w + radius {
                    let dx = (sidebar_w + radius) as f32 - (px as f32 + 0.5);
                    let dy = radius as f32 - (py as f32 + 0.5);
                    let signed_dist = (dx * dx + dy * dy).sqrt() - r_f;
                    let cov = (0.5 - signed_dist).clamp(0.0, 1.0);
                    if cov <= 0.0 {
                        continue;
                    }
                    let (cr, cg, cb) = if signed_dist > -1.0 {
                        let t = (signed_dist + 1.0).clamp(0.0, 1.0);
                        (
                            (bg_r as f32 + (bd_r as f32 - bg_r as f32) * t).round() as u8,
                            (bg_g as f32 + (bd_g as f32 - bg_g as f32) * t).round() as u8,
                            (bg_b as f32 + (bd_b as f32 - bg_b as f32) * t).round() as u8,
                        )
                    } else {
                        (bg_r, bg_g, bg_b)
                    };
                    let a = (cov * 255.0).round() as u32;
                    let pr = ((cr as u32 * a + 127) / 255) & 0xFF;
                    let pg = ((cg as u32 * a + 127) / 255) & 0xFF;
                    let pb = ((cb as u32 * a + 127) / 255) & 0xFF;
                    *ptr = (a << 24) | (pr << 16) | (pg << 8) | pb;
                } else if (py == 0 && px >= sidebar_w + radius) || (px == sx0 && py >= radius) {
                    *ptr = bd_pixel;
                } else {
                    *ptr = bg_pixel;
                }
            }
        }
    }

    /// Composites every non-opaque (premultiplied) pixel over an opaque base colour (BGR).
    pub unsafe fn flatten_onto_opaque(&self, base_bgr: u32) {
        let base_r = base_bgr & 0xFF;
        let base_g = (base_bgr >> 8) & 0xFF;
        let base_b = (base_bgr >> 16) & 0xFF;
        let count = (self.w as usize) * (self.h as usize);
        for i in 0..count {
            let ptr = self.bits.add(i);
            let px = *ptr;
            let a = (px >> 24) & 0xFF;
            if a == 255 {
                continue;
            }
            let inv = 255 - a;
            let r = (((px >> 16) & 0xFF) + (base_r * inv + 127) / 255).min(255);
            let g = (((px >> 8) & 0xFF) + (base_g * inv + 127) / 255).min(255);
            let b = ((px & 0xFF) + (base_b * inv + 127) / 255).min(255);
            *ptr = 0xFF00_0000 | (r << 16) | (g << 8) | b;
        }
    }

    /// Makes the plain page background of the content panel (pixels exactly equal to the opaque
    /// `bg_bgr`) translucent, so the Mica backdrop glows through it more darkly than through the
    /// rail; cards, text and controls stay opaque. Call after `stamp_content_panel_alpha`.
    pub unsafe fn tint_content_background(&self, sidebar_w: i32, bg_bgr: u32, alpha: u32) {
        let (r, g, b) = (bg_bgr & 0xFF, (bg_bgr >> 8) & 0xFF, (bg_bgr >> 16) & 0xFF);
        let opaque = 0xFF00_0000 | (r << 16) | (g << 8) | b;
        let a = alpha.min(255);
        let pm = |c: u32| (c * a + 127) / 255;
        let translucent = (a << 24) | (pm(r) << 16) | (pm(g) << 8) | pm(b);
        for py in 0..self.h {
            let row_off = (py as usize) * (self.w as usize);
            for px in sidebar_w.max(0)..self.w {
                let ptr = self.bits.add(row_off + (px as usize));
                if *ptr == opaque {
                    *ptr = translucent;
                }
            }
        }
    }

    pub unsafe fn stamp_content_panel_alpha(&self, sidebar_w: i32, radius: i32) {
        let sx0 = sidebar_w.max(0);
        let r_inner = (radius as f32 - 1.5).max(0.0);
        let r_inner_sq = r_inner * r_inner;
        for py in 0..self.h {
            let row_off = (py as usize) * (self.w as usize);
            let in_top_arc_y = py < radius;
            for px in sx0..self.w {
                if in_top_arc_y && px < sidebar_w + radius {
                    let dx = (sidebar_w + radius) as f32 - (px as f32 + 0.5);
                    let dy_arc = radius as f32 - (py as f32 + 0.5);
                    if dx * dx + dy_arc * dy_arc > r_inner_sq {
                        continue;
                    }
                }
                let ptr = self.bits.add(row_off + (px as usize));
                *ptr |= 0xFF00_0000;
            }
        }
    }

    /// Moves every pixel inside `rc` toward the opaque colour `bg_bgr` by `1 - alpha/255`
    /// (used for the page "drill-in" fade without re-rendering into a separate surface).
    pub unsafe fn fade_rect_toward(&self, rc: &RECT, bg_bgr: u32, alpha: u8) {
        if alpha == 255 {
            return;
        }
        let x0 = rc.left.max(0);
        let x1 = rc.right.min(self.w);
        let y0 = rc.top.max(0);
        let y1 = rc.bottom.min(self.h);
        let t = alpha as u32;
        let inv = 255 - t;
        let (br, bg, bb) = (bg_bgr & 0xFF, (bg_bgr >> 8) & 0xFF, (bg_bgr >> 16) & 0xFF);
        for py in y0..y1 {
            let row = (py as usize) * (self.w as usize);
            for px in x0..x1 {
                let ptr = self.bits.add(row + px as usize);
                let v = *ptr;
                let a = (v >> 24) & 0xFF;
                let r = (((v >> 16) & 0xFF) * t + br * inv * a / 255 + 127) / 255;
                let g = (((v >> 8) & 0xFF) * t + bg * inv * a / 255 + 127) / 255;
                let b = ((v & 0xFF) * t + bb * inv * a / 255 + 127) / 255;
                *ptr = (a << 24) | (r.min(255) << 16) | (g.min(255) << 8) | b.min(255);
            }
        }
    }

    /// Marks a rectangle fully opaque (after plain GDI drawing, which leaves alpha at 0).
    pub unsafe fn stamp_opaque_rect(&self, rc: &RECT) {
        let x0 = rc.left.max(0);
        let x1 = rc.right.min(self.w);
        let y0 = rc.top.max(0);
        let y1 = rc.bottom.min(self.h);
        for py in y0..y1 {
            let row = (py as usize) * (self.w as usize);
            for px in x0..x1 {
                *self.bits.add(row + px as usize) |= 0xFF00_0000;
            }
        }
    }
}
