use crate::config::config_dir;
use image::codecs::ico::{IcoEncoder, IcoFrame};
use image::imageops::FilterType;
use image::ExtendedColorType;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub fn icons_dir() -> PathBuf {
    config_dir().join("icons")
}

/// Converts any supported image (PNG, JPG, BMP, WebP, GIF, ICO) into a multi-resolution
/// Windows `.ico` file (256x256, 48x48, 32x32, 16x16) saved under `%LOCALAPPDATA%\CustomerFD\icons\`.
pub fn import_image_as_ico(src_path: &Path) -> Result<PathBuf, String> {
    let img = image::open(src_path).map_err(|e| format!("無法讀取圖片: {e}"))?;
    let dir = icons_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("無法建立圖示目錄: {e}"))?;

    let stem = src_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("custom_icon");
    let sanitized: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut dest = dir.join(format!("{sanitized}.ico"));
    let mut idx = 1usize;
    while dest.exists() {
        dest = dir.join(format!("{sanitized}_{idx}.ico"));
        idx += 1;
    }

    let sizes = [256u32, 48, 32, 16];
    let mut frames = Vec::with_capacity(sizes.len());
    for &sz in &sizes {
        let resized = img.resize_exact(sz, sz, FilterType::Lanczos3).to_rgba8();
        let frame = IcoFrame::as_png(resized.as_raw(), sz, sz, ExtendedColorType::Rgba8)
            .map_err(|e| format!("建立 ICO 影格失敗: {e}"))?;
        frames.push(frame);
    }

    let file = File::create(&dest).map_err(|e| format!("無法建立 ICO 檔案: {e}"))?;
    let writer = BufWriter::new(file);
    let encoder = IcoEncoder::new(writer);
    encoder
        .encode_images(&frames)
        .map_err(|e| format!("寫入 ICO 失敗: {e}"))?;

    Ok(dest)
}

pub fn list_custom_icons() -> Vec<PathBuf> {
    let dir = icons_dir();
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.eq_ignore_ascii_case("ico"))
                    .unwrap_or(false)
        })
        .collect();
    out.sort();
    out
}

pub fn delete_custom_icon(path: &Path) -> Result<(), String> {
    let dir = icons_dir();
    if !path.starts_with(&dir) {
        return Err("僅允許刪除自訂圖示目錄內的檔案".to_string());
    }
    std::fs::remove_file(path).map_err(|e| format!("刪除圖示失敗: {e}"))
}

/// Loads an `.ico` or image file and returns a premultiplied BGRA buffer of `size x size` for preview.
pub fn load_icon_preview_bgra(path: &Path, size: u32) -> Option<Vec<u8>> {
    if size == 0 {
        return None;
    }
    let img = image::open(path).ok()?;
    let resized = img
        .resize_exact(size, size, FilterType::Lanczos3)
        .to_rgba8();
    let mut bgra = vec![0u8; (size * size * 4) as usize];
    for (i, px) in resized.pixels().enumerate() {
        let [r, g, b, a] = px.0;
        let af = a as f32 / 255.0;
        let idx = i * 4;
        bgra[idx] = (b as f32 * af).round() as u8;
        bgra[idx + 1] = (g as f32 * af).round() as u8;
        bgra[idx + 2] = (r as f32 * af).round() as u8;
        bgra[idx + 3] = a;
    }
    Some(bgra)
}
