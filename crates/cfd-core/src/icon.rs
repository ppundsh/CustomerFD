use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::UI::WindowsAndMessaging::{CreateIconIndirect, HICON, ICONINFO};

/// Creates a clean 32x32 folder-themed HICON (fallback window icon).
///
/// # Safety
/// Plain GDI/User32 object creation; the caller owns the returned icon.
pub unsafe fn create_app_hicon(accent_rgb: u32) -> Option<HICON> {
    let size = 32i32;
    let mut bgra = vec![0u8; (size * size * 4) as usize];

    let r = ((accent_rgb >> 16) & 0xFF) as f32;
    let g = ((accent_rgb >> 8) & 0xFF) as f32;
    let b = (accent_rgb & 0xFF) as f32;

    // Darker shade for folder back tab
    let r_dark = (r * 0.72).round();
    let g_dark = (g * 0.72).round();
    let b_dark = (b * 0.72).round();

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Rounded folder tab (top-left): x in [3..15], y in [5..11]
            let in_tab = (3.0..=15.0).contains(&fx) && (5.0..=12.0).contains(&fy);
            // Folder back body: x in [3..29], y in [8..26]
            let in_back = (3.0..=29.0).contains(&fx) && (8.0..=26.0).contains(&fy);
            // Folder front body: x in [3..29], y in [11..27]
            let in_front = (3.0..=29.0).contains(&fx) && (11.0..=27.0).contains(&fy);

            let idx = ((y * size + x) * 4) as usize;
            if in_front {
                // Corner rounding check
                let corner = (fx < 5.0 && fy < 13.0)
                    || (fx > 27.0 && fy < 13.0)
                    || (fx < 5.0 && fy > 25.0)
                    || (fx > 27.0 && fy > 25.0);
                if !corner {
                    bgra[idx] = b as u8;
                    bgra[idx + 1] = g as u8;
                    bgra[idx + 2] = r as u8;
                    bgra[idx + 3] = 255;
                }
            } else if in_tab || in_back {
                bgra[idx] = b_dark as u8;
                bgra[idx + 1] = g_dark as u8;
                bgra[idx + 2] = r_dark as u8;
                bgra[idx + 3] = 255;
            }
        }
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let dc = CreateCompatibleDC(HDC::default());
    let Ok(hbmp_color) = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, None, 0) else {
        let _ = DeleteDC(dc);
        return None;
    };
    if !bits_ptr.is_null() {
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits_ptr.cast::<u8>(), bgra.len());
    }

    let mask_bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 1,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut mask_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp_mask: HBITMAP =
        CreateDIBSection(dc, &mask_bmi, DIB_RGB_COLORS, &mut mask_ptr, None, 0).unwrap_or_default();

    let icon_info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: hbmp_mask,
        hbmColor: hbmp_color,
    };
    let hicon = CreateIconIndirect(&icon_info).ok();

    let _ = DeleteObject(hbmp_color);
    if !hbmp_mask.is_invalid() {
        let _ = DeleteObject(hbmp_mask);
    }
    let _ = DeleteDC(dc);
    hicon
}
