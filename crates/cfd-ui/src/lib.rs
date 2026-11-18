//! Windows 11 Fluent drawing toolkit shared by `cfd-manager` and `cfd-installer`.
//!
//! The visual rules come from cbxshell-rs (`doc/ui-design.zh-TW.md`): 96-DPI design pixels
//! scaled with [`scale`], tokens from [`theme`], anti-aliased shapes from [`render_aa`], the
//! translucent back buffer in [`dib`], and the Mica / Acrylic backdrop in [`backdrop`].

pub mod anim;
pub mod backdrop;
pub mod dib;
pub mod gdi;
pub mod render_aa;
pub mod theme;
pub mod titlebar;
pub mod widgets;
pub mod wstr;

/// Converts 96-DPI design pixels to physical pixels: `(px × dpi + 48) / 96`.
#[inline]
pub fn scale(px: i32, dpi: u32) -> i32 {
    (px * dpi as i32 + 48) / 96
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_rounds_half_up() {
        assert_eq!(scale(10, 96), 10);
        assert_eq!(scale(10, 144), 15);
        assert_eq!(scale(195, 120), 244);
        assert_eq!(scale(1, 144), 2);
    }

    #[test]
    fn lerp_endpoints() {
        assert_eq!(theme::lerp_rgb(0x000000, 0xFFFFFF, 0.0), 0x000000);
        assert_eq!(theme::lerp_rgb(0x000000, 0xFFFFFF, 1.0), 0xFFFFFF);
        assert_eq!(theme::rgb_to_bgr(0x112233), 0x332211);
    }

    #[test]
    fn approach_snaps() {
        let mut v = 0.0;
        let mut frames = 0;
        while anim::approach(&mut v, 1.0) {
            frames += 1;
            assert!(frames < 50);
        }
        assert_eq!(v, 1.0);
    }
}
