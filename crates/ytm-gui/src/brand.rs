use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

pub const APP_NAME: &str = "Dalga";
pub const TAGLINE: &str = "Dalga — Native Music & Media Player";
pub const WINDOW_TITLE: &str = "Dalga";
pub const TRAY_TITLE: &str = "Dalga";
pub const MPRIS_DISPLAY_NAME: &str = "Dalga";

pub const LOGO_DARK_PNG: &[u8] = include_bytes!("../../../assets/brand/dalga-dark-128.png");
pub const LOGO_LIGHT_PNG: &[u8] = include_bytes!("../../../assets/brand/dalga-light-128.png");
pub const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/brand/dalga-icon-256.png");

/// Decode the embedded 256x256 application icon into `egui::IconData`.
#[must_use]
pub fn load_window_icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(APP_ICON_PNG).ok()?.into_rgba8();
    let (width, height) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width,
        height,
    })
}

/// Return the cached theme-appropriate Dalga logo texture for UI chrome.
#[must_use]
pub fn logo_texture(ctx: &egui::Context) -> Option<TextureHandle> {
    let dark = ctx.style().visuals.dark_mode;
    let key = egui::Id::new(if dark {
        "dalga_brand_logo_dark"
    } else {
        "dalga_brand_logo_light"
    });
    if let Some(cached) = ctx.data(|d| d.get_temp::<TextureHandle>(key)) {
        return Some(cached);
    }
    let bytes = if dark { LOGO_DARK_PNG } else { LOGO_LIGHT_PNG };
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ctx.load_texture(
        if dark {
            "dalga-logo-dark"
        } else {
            "dalga-logo-light"
        },
        color,
        TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(key, tex.clone()));
    Some(tex)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn embedded_window_icon_decodes_256px() {
        let icon = load_window_icon().expect("decode Dalga window icon");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }

    #[test]
    fn brand_name_is_centralized_in_brand_module() {
        let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let needle = "\"Dalga\"";
        let mut stack = vec![src_dir];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name == "brand.rs" || name.ends_with("_tests.rs") {
                        continue;
                    }
                    let text = std::fs::read_to_string(&path).expect("read source file");
                    assert!(
                        !text.contains(needle),
                        "hardcoded brand literal {needle} found in {}",
                        path.display()
                    );
                }
            }
        }
    }
}
