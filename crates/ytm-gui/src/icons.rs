use eframe::egui::{self, Color32, ImageSource, Vec2};

/// Embedded Lucide SVG icon catalog (zero font-dependent glyphs or emojis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Play,
    Pause,
    Stop,
    Prev,
    Next,
    Shuffle,
    Repeat,
    RepeatOne,
    Heart,
    VolumeOn,
    VolumeMute,
    Radio,
    Music,
    ListMusic,
    Mic,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Sync,
    Refresh,
    Search,
    Close,
    Check,
    Warn,
    Lock,
    Plus,
    Minus,
    Trash,
    Edit,
    ExternalLink,
    Copy,
    DragHandle,
    Star,
    Filter,
    Sparkles,
    Sun,
    Moon,
    Monitor,
    Settings,
    User,
    MiniPlayer,
    Expand,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    Globe,
    Layers,
    Shield,
    Activity,
    Tags,
    GitCompare,
    Bell,
}

impl Icon {
    pub const ALL: [Self; 53] = [
        Self::Play,
        Self::Pause,
        Self::Stop,
        Self::Prev,
        Self::Next,
        Self::Shuffle,
        Self::Repeat,
        Self::RepeatOne,
        Self::Heart,
        Self::VolumeOn,
        Self::VolumeMute,
        Self::Radio,
        Self::Music,
        Self::ListMusic,
        Self::Mic,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::ArrowUp,
        Self::ArrowDown,
        Self::Sync,
        Self::Refresh,
        Self::Search,
        Self::Close,
        Self::Check,
        Self::Warn,
        Self::Lock,
        Self::Plus,
        Self::Minus,
        Self::Trash,
        Self::Edit,
        Self::ExternalLink,
        Self::Copy,
        Self::DragHandle,
        Self::Star,
        Self::Filter,
        Self::Sparkles,
        Self::Sun,
        Self::Moon,
        Self::Monitor,
        Self::Settings,
        Self::User,
        Self::MiniPlayer,
        Self::Expand,
        Self::ChevronDown,
        Self::ChevronRight,
        Self::ChevronUp,
        Self::Globe,
        Self::Layers,
        Self::Shield,
        Self::Activity,
        Self::Tags,
        Self::GitCompare,
        Self::Bell,
    ];

    /// Stable URI key used by `egui_extras` SVG loader cache.
    #[must_use]
    pub const fn uri(self) -> &'static str {
        match self {
            Self::Play => "bytes://lucide/play.svg",
            Self::Pause => "bytes://lucide/pause.svg",
            Self::Stop => "bytes://lucide/square.svg",
            Self::Prev => "bytes://lucide/skip-back.svg",
            Self::Next => "bytes://lucide/skip-forward.svg",
            Self::Shuffle => "bytes://lucide/shuffle.svg",
            Self::Repeat => "bytes://lucide/repeat.svg",
            Self::RepeatOne => "bytes://lucide/repeat-1.svg",
            Self::Heart => "bytes://lucide/heart.svg",
            Self::VolumeOn => "bytes://lucide/volume-2.svg",
            Self::VolumeMute => "bytes://lucide/volume-x.svg",
            Self::Radio => "bytes://lucide/radio.svg",
            Self::Music => "bytes://lucide/music.svg",
            Self::ListMusic => "bytes://lucide/list-music.svg",
            Self::Mic => "bytes://lucide/mic.svg",
            Self::ArrowLeft => "bytes://lucide/arrow-left.svg",
            Self::ArrowRight => "bytes://lucide/arrow-right.svg",
            Self::ArrowUp => "bytes://lucide/arrow-up.svg",
            Self::ArrowDown => "bytes://lucide/arrow-down.svg",
            Self::Sync => "bytes://lucide/arrow-up-down.svg",
            Self::Refresh => "bytes://lucide/refresh-cw.svg",
            Self::Search => "bytes://lucide/search.svg",
            Self::Close => "bytes://lucide/x.svg",
            Self::Check => "bytes://lucide/check.svg",
            Self::Warn => "bytes://lucide/triangle-alert.svg",
            Self::Lock => "bytes://lucide/lock.svg",
            Self::Plus => "bytes://lucide/plus.svg",
            Self::Minus => "bytes://lucide/minus.svg",
            Self::Trash => "bytes://lucide/trash-2.svg",
            Self::Edit => "bytes://lucide/pen-line.svg",
            Self::ExternalLink => "bytes://lucide/external-link.svg",
            Self::Copy => "bytes://lucide/copy.svg",
            Self::DragHandle => "bytes://lucide/grip-vertical.svg",
            Self::Star => "bytes://lucide/star.svg",
            Self::Filter => "bytes://lucide/list-filter.svg",
            Self::Sparkles => "bytes://lucide/sparkles.svg",
            Self::Sun => "bytes://lucide/sun.svg",
            Self::Moon => "bytes://lucide/moon.svg",
            Self::Monitor => "bytes://lucide/monitor.svg",
            Self::Settings => "bytes://lucide/settings.svg",
            Self::User => "bytes://lucide/user.svg",
            Self::MiniPlayer => "bytes://lucide/picture-in-picture-2.svg",
            Self::Expand => "bytes://lucide/maximize-2.svg",
            Self::ChevronDown => "bytes://lucide/chevron-down.svg",
            Self::ChevronRight => "bytes://lucide/chevron-right.svg",
            Self::ChevronUp => "bytes://lucide/chevron-up.svg",
            Self::Globe => "bytes://lucide/globe.svg",
            Self::Layers => "bytes://lucide/layers.svg",
            Self::Shield => "bytes://lucide/shield.svg",
            Self::Activity => "bytes://lucide/activity.svg",
            Self::Tags => "bytes://lucide/tags.svg",
            Self::GitCompare => "bytes://lucide/git-compare.svg",
            Self::Bell => "bytes://lucide/bell.svg",
        }
    }

    /// Raw embedded SVG bytes for this icon.
    #[must_use]
    pub const fn svg_bytes(self) -> &'static [u8] {
        match self {
            Self::Play => include_bytes!("../../../assets/icons/play.svg"),
            Self::Pause => include_bytes!("../../../assets/icons/pause.svg"),
            Self::Stop => include_bytes!("../../../assets/icons/square.svg"),
            Self::Prev => include_bytes!("../../../assets/icons/skip-back.svg"),
            Self::Next => include_bytes!("../../../assets/icons/skip-forward.svg"),
            Self::Shuffle => include_bytes!("../../../assets/icons/shuffle.svg"),
            Self::Repeat => include_bytes!("../../../assets/icons/repeat.svg"),
            Self::RepeatOne => include_bytes!("../../../assets/icons/repeat-1.svg"),
            Self::Heart => include_bytes!("../../../assets/icons/heart.svg"),
            Self::VolumeOn => include_bytes!("../../../assets/icons/volume-2.svg"),
            Self::VolumeMute => include_bytes!("../../../assets/icons/volume-x.svg"),
            Self::Radio => include_bytes!("../../../assets/icons/radio.svg"),
            Self::Music => include_bytes!("../../../assets/icons/music.svg"),
            Self::ListMusic => include_bytes!("../../../assets/icons/list-music.svg"),
            Self::Mic => include_bytes!("../../../assets/icons/mic.svg"),
            Self::ArrowLeft => include_bytes!("../../../assets/icons/arrow-left.svg"),
            Self::ArrowRight => include_bytes!("../../../assets/icons/arrow-right.svg"),
            Self::ArrowUp => include_bytes!("../../../assets/icons/arrow-up.svg"),
            Self::ArrowDown => include_bytes!("../../../assets/icons/arrow-down.svg"),
            Self::Sync => include_bytes!("../../../assets/icons/arrow-up-down.svg"),
            Self::Refresh => include_bytes!("../../../assets/icons/refresh-cw.svg"),
            Self::Search => include_bytes!("../../../assets/icons/search.svg"),
            Self::Close => include_bytes!("../../../assets/icons/x.svg"),
            Self::Check => include_bytes!("../../../assets/icons/check.svg"),
            Self::Warn => include_bytes!("../../../assets/icons/triangle-alert.svg"),
            Self::Lock => include_bytes!("../../../assets/icons/lock.svg"),
            Self::Plus => include_bytes!("../../../assets/icons/plus.svg"),
            Self::Minus => include_bytes!("../../../assets/icons/minus.svg"),
            Self::Trash => include_bytes!("../../../assets/icons/trash-2.svg"),
            Self::Edit => include_bytes!("../../../assets/icons/pen-line.svg"),
            Self::ExternalLink => include_bytes!("../../../assets/icons/external-link.svg"),
            Self::Copy => include_bytes!("../../../assets/icons/copy.svg"),
            Self::DragHandle => include_bytes!("../../../assets/icons/grip-vertical.svg"),
            Self::Star => include_bytes!("../../../assets/icons/star.svg"),
            Self::Filter => include_bytes!("../../../assets/icons/list-filter.svg"),
            Self::Sparkles => include_bytes!("../../../assets/icons/sparkles.svg"),
            Self::Sun => include_bytes!("../../../assets/icons/sun.svg"),
            Self::Moon => include_bytes!("../../../assets/icons/moon.svg"),
            Self::Monitor => include_bytes!("../../../assets/icons/monitor.svg"),
            Self::Settings => include_bytes!("../../../assets/icons/settings.svg"),
            Self::User => include_bytes!("../../../assets/icons/user.svg"),
            Self::MiniPlayer => include_bytes!("../../../assets/icons/picture-in-picture-2.svg"),
            Self::Expand => include_bytes!("../../../assets/icons/maximize-2.svg"),
            Self::ChevronDown => include_bytes!("../../../assets/icons/chevron-down.svg"),
            Self::ChevronRight => include_bytes!("../../../assets/icons/chevron-right.svg"),
            Self::ChevronUp => include_bytes!("../../../assets/icons/chevron-up.svg"),
            Self::Globe => include_bytes!("../../../assets/icons/globe.svg"),
            Self::Layers => include_bytes!("../../../assets/icons/layers.svg"),
            Self::Shield => include_bytes!("../../../assets/icons/shield.svg"),
            Self::Activity => include_bytes!("../../../assets/icons/activity.svg"),
            Self::Tags => include_bytes!("../../../assets/icons/tags.svg"),
            Self::GitCompare => include_bytes!("../../../assets/icons/git-compare.svg"),
            Self::Bell => include_bytes!("../../../assets/icons/bell.svg"),
        }
    }

    /// Build a theme-tinted `egui::ImageSource` ready for `egui::Image` or `egui::Button`.
    #[must_use]
    pub const fn source(self) -> ImageSource<'static> {
        ImageSource::Bytes {
            uri: std::borrow::Cow::Borrowed(self.uri()),
            bytes: egui::load::Bytes::Static(self.svg_bytes()),
        }
    }

    /// Build a sized, theme-tinted `egui::Image` widget.
    pub fn image(self, size: f32, tint: Color32) -> egui::Image<'static> {
        egui::Image::new(self.source())
            .fit_to_exact_size(Vec2::splat(size))
            .tint(tint)
    }
}

/// Render a standalone icon at `size` tinted by `tint`.
pub fn paint_icon(ui: &mut egui::Ui, icon: Icon, size: f32, tint: Color32) -> egui::Response {
    ui.add(icon.image(size, tint))
}

/// Render a clickable icon button tinted by theme text color.
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    enabled: bool,
    tooltip: &str,
) -> egui::Response {
    let tint = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.add_enabled(enabled, egui::Button::image(icon.image(size, tint)))
        .on_hover_text(tooltip)
}

/// Render a compact icon + text button tinted by theme text color.
pub fn icon_text_button(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    size: f32,
    enabled: bool,
) -> egui::Response {
    let tint = if enabled {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.add_enabled(
        enabled,
        egui::Button::image_and_text(icon.image(size, tint), label),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_icon_variant_maps_to_unique_valid_svg_and_rasterizes() {
        let mut uris = HashSet::new();
        for icon in Icon::ALL {
            assert!(uris.insert(icon.uri()), "duplicate URI for {icon:?}");
            let bytes = icon.svg_bytes();
            let text = std::str::from_utf8(bytes).expect("utf-8 svg");
            assert!(text.contains("<svg"), "{icon:?} missing <svg tag");
            let img = egui_extras::image::load_svg_bytes_with_size(
                bytes,
                Some(egui::SizeHint::Size(16, 16)),
            )
            .expect("rasterize svg");
            assert_eq!(img.size, [16, 16]);
        }
    }
}
