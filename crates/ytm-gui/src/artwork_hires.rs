use eframe::egui::{Color32, ColorImage, Pos2, Rect, Vec2};
use image::imageops::FilterType;

use crate::theme_accent::extract_dominant_accent_rgba;

/// Rewrites low-resolution Google/YouTube Music image URLs (`=w60-h60`, `=s120`, etc.) to `1024px` variants.
#[must_use]
pub fn upgrade_artwork_url_hires(url: &str) -> String {
    if url.contains("googleusercontent.com") || url.contains("yt3.ggpht.com") {
        if let Some(eq_idx) = url.rfind('=') {
            let after_last_slash = url.rfind('/').is_none_or(|s| eq_idx > s);
            if after_last_slash {
                let suffix = &url[eq_idx + 1..];
                if suffix.starts_with('w') || suffix.starts_with('s') {
                    return format!("{}=s1024", &url[..eq_idx]);
                }
            }
        }
    } else if url.contains("i.ytimg.com/vi/") {
        for small in ["/default.jpg", "/mqdefault.jpg"] {
            if let Some(prefix) = url.strip_suffix(small) {
                return format!("{prefix}/hqdefault.jpg");
            }
        }
    }
    url.to_string()
}

/// Computes aspect-preserving center-crop UV coordinates (`object-fit: cover`) so wide banners never squish in square frames.
#[must_use]
pub fn cover_crop_uv(tex_size: Vec2, target_size: Vec2) -> Rect {
    if tex_size.x <= 0.0 || tex_size.y <= 0.0 || target_size.x <= 0.0 || target_size.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let src_ratio = tex_size.x / tex_size.y;
    let dst_ratio = target_size.x / target_size.y;
    if (src_ratio - dst_ratio).abs() < 0.01 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    if src_ratio > dst_ratio {
        let keep_w = (dst_ratio / src_ratio).clamp(0.05, 1.0);
        let pad_x = (1.0 - keep_w) * 0.5;
        Rect::from_min_max(Pos2::new(pad_x, 0.0), Pos2::new(1.0 - pad_x, 1.0))
    } else {
        let keep_h = (src_ratio / dst_ratio).clamp(0.05, 1.0);
        let pad_y = (1.0 - keep_h) * 0.5;
        Rect::from_min_max(Pos2::new(0.0, pad_y), Pos2::new(1.0, 1.0 - pad_y))
    }
}

pub(crate) fn decode_image_with_max_dim(
    bytes: &[u8],
    max_dim: u32,
) -> Option<(ColorImage, Option<Color32>)> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = if img.width() > max_dim || img.height() > max_dim {
        img.resize(max_dim, max_dim, FilterType::Lanczos3)
    } else {
        img
    };
    let rgba = resized.to_rgba8();
    let accent = extract_dominant_accent_rgba(rgba.as_raw(), rgba.width(), rgba.height());
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some((
        ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()),
        accent,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_artwork_url_rewrites_google_and_ytimg_sizes() {
        assert_eq!(
            upgrade_artwork_url_hires("https://lh3.googleusercontent.com/abc=w60-h60-l90-rj"),
            "https://lh3.googleusercontent.com/abc=s1024"
        );
        assert_eq!(
            upgrade_artwork_url_hires("https://yt3.ggpht.com/xyz=s120-c-k"),
            "https://yt3.ggpht.com/xyz=s1024"
        );
        assert_eq!(
            upgrade_artwork_url_hires("https://i.ytimg.com/vi/abc123/mqdefault.jpg"),
            "https://i.ytimg.com/vi/abc123/hqdefault.jpg"
        );
        assert_eq!(
            upgrade_artwork_url_hires("https://example.com/cover.png"),
            "https://example.com/cover.png"
        );
    }

    #[test]
    fn cover_crop_uv_crops_wide_banner_for_square_avatar_without_distortion() {
        let uv = cover_crop_uv(Vec2::new(1600.0, 900.0), Vec2::new(112.0, 112.0));
        assert!((uv.width() - (9.0 / 16.0)).abs() < 1e-4);
        assert!((uv.height() - 1.0).abs() < 1e-4);
        assert!((uv.center().x - 0.5).abs() < 1e-4);
    }
}
