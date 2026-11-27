use eframe::egui::Color32;

/// Extract a dominant chromatic accent color from raw RGBA8 pixels (intended for off-thread decode).
/// Returns `None` for grayscale, near-black, near-white, or empty images so callers fall back cleanly.
#[must_use]
pub fn extract_dominant_accent_rgba(rgba: &[u8], width: u32, height: u32) -> Option<Color32> {
    if width == 0 || height == 0 || rgba.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let pixel_count = (width as usize) * (height as usize);
    let stride = (pixel_count / 2048).max(1);

    let mut bins = [(0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64); 12];
    for idx in (0..pixel_count).step_by(stride) {
        let base = idx * 4;
        let a = rgba[base + 3];
        if a < 128 {
            continue;
        }
        let r = f64::from(rgba[base]) / 255.0;
        let g = f64::from(rgba[base + 1]) / 255.0;
        let b = f64::from(rgba[base + 2]) / 255.0;
        let (h, s, l) = rgb_to_hsl(r, g, b);
        if s < 0.18 || !(0.12..=0.90).contains(&l) {
            continue;
        }
        let weight = s * (1.0 - (2.0 * l - 1.0).abs() * 0.65);
        let bin = ((h * 12.0).floor() as usize).min(11);
        bins[bin].0 += r * weight;
        bins[bin].1 += g * weight;
        bins[bin].2 += b * weight;
        bins[bin].3 += weight;
    }

    let best = bins
        .iter()
        .filter(|b| b.3 > 0.0)
        .max_by(|a, b| a.3.total_cmp(&b.3))?;
    let r = ((best.0 / best.3) * 255.0).round().clamp(0.0, 255.0) as u8;
    let g = ((best.1 / best.3) * 255.0).round().clamp(0.0, 255.0) as u8;
    let b = ((best.2 / best.3) * 255.0).round().clamp(0.0, 255.0) as u8;
    Some(Color32::from_rgb(r, g, b))
}

/// Clamp lightness/saturation of a raw dominant color for crisp contrast on dark or light surfaces.
/// Returns `(accent_primary, accent_selection, accent_text)`.
#[must_use]
pub fn tune_accent_for_mode(base: Color32, dark_mode: bool) -> (Color32, Color32, Color32) {
    let (h, s, l) = rgb_to_hsl(
        f64::from(base.r()) / 255.0,
        f64::from(base.g()) / 255.0,
        f64::from(base.b()) / 255.0,
    );
    let clamped_s = s.clamp(0.45, 0.85);
    let clamped_l = if dark_mode {
        l.clamp(0.54, 0.68)
    } else {
        l.clamp(0.32, 0.42)
    };
    let primary = hsl_to_rgb(h, clamped_s, clamped_l);
    let sel_l = if dark_mode { 0.22 } else { 0.88 };
    let selection = hsl_to_rgb(h, (clamped_s * 0.65).clamp(0.25, 0.65), sel_l);
    let text = best_contrast_bw(primary);
    (primary, selection, text)
}

/// Smoothly interpolate between two `Color32` values (`t` in `0.0..=1.0`).
#[must_use]
pub fn lerp_color(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |a: u8, b: u8| -> u8 {
        (f32::from(a) + (f32::from(b) - f32::from(a)) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(
        mix(from.r(), to.r()),
        mix(from.g(), to.g()),
        mix(from.b(), to.b()),
    )
}

/// Compute WCAG 2.1 relative luminance in `[0.0, 1.0]`.
#[must_use]
pub fn relative_luminance(c: Color32) -> f64 {
    let chan = |v: u8| -> f64 {
        let s = f64::from(v) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * chan(c.r()) + 0.7152 * chan(c.g()) + 0.0722 * chan(c.b())
}

/// Compute WCAG 2.1 contrast ratio between two opaque colors (`>= 1.0`).
#[must_use]
pub fn contrast_ratio(a: Color32, b: Color32) -> f64 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[must_use]
pub fn best_contrast_bw(bg: Color32) -> Color32 {
    let dark_ink = Color32::from_rgb(14, 16, 20);
    let white_ink = Color32::from_rgb(252, 253, 255);
    if contrast_ratio(bg, dark_ink) >= contrast_ratio(bg, white_ink) {
        dark_ink
    } else {
        white_ink
    }
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) * 0.5;
    if (max - min).abs() < 1e-9 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let mut h = if (max - r).abs() < 1e-9 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-9 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    h /= 6.0;
    (h.rem_euclid(1.0), s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> Color32 {
    if s <= 1e-9 {
        let v = (l * 255.0).round().clamp(0.0, 255.0) as u8;
        return Color32::from_rgb(v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue2rgb = |mut t: f64| -> u8 {
        t = t.rem_euclid(1.0);
        let c = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (c * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Color32::from_rgb(hue2rgb(h + 1.0 / 3.0), hue2rgb(h), hue2rgb(h - 1.0 / 3.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dominant_accent_extracts_stable_hue_and_rejects_grayscale() {
        let mut blue_img = Vec::with_capacity(16 * 16 * 4);
        for _ in 0..(16 * 16) {
            blue_img.extend_from_slice(&[35, 115, 225, 255]);
        }
        let col = extract_dominant_accent_rgba(&blue_img, 16, 16).expect("dominant blue");
        assert!(col.b() > col.r() && col.b() > col.g());

        let mut gray_img = Vec::with_capacity(8 * 8 * 4);
        for _ in 0..(8 * 8) {
            gray_img.extend_from_slice(&[90, 90, 90, 255]);
        }
        assert!(extract_dominant_accent_rgba(&gray_img, 8, 8).is_none());
    }

    #[test]
    fn tune_accent_for_mode_maintains_readable_contrast() {
        let raw = Color32::from_rgb(240, 220, 40);
        let (dark_p, _, dark_txt) = tune_accent_for_mode(raw, true);
        assert!(contrast_ratio(dark_p, dark_txt) >= 4.5);
        let (light_p, _, light_txt) = tune_accent_for_mode(raw, false);
        assert!(contrast_ratio(light_p, light_txt) >= 4.5);
    }
}
