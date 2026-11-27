use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

use crate::contracts::{AccentPalette, ThemeMode};
use crate::theme_accent::{lerp_color, tune_accent_for_mode};

/// Legacy dark constants retained as aliases for call sites migrating to `Palette`.
pub const BG_CANVAS: Color32 = Color32::from_rgb(12, 14, 18);
pub const BG_PANEL: Color32 = Color32::from_rgb(18, 21, 27);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(27, 32, 41);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(48, 56, 68);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(234, 239, 247);
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(76, 168, 122);
pub const ACCENT_SELECTION: Color32 = Color32::from_rgb(36, 74, 58);

/// Complete surface + semantic + accent palette for Dalga's Ultra-Dark and Light themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub dark_mode: bool,
    pub bg_canvas: Color32,
    pub bg_panel: Color32,
    pub bg_elevated: Color32,
    pub bg_faint: Color32,
    pub bg_extreme: Color32,
    pub bg_hover: Color32,
    pub border_subtle: Color32,
    pub text_primary: Color32,
    pub text_muted: Color32,
    pub accent_primary: Color32,
    pub accent_selection: Color32,
    pub accent_text: Color32,
    pub status_ok: Color32,
    pub status_warn: Color32,
    pub status_err: Color32,
}

impl Palette {
    /// Static Ultra-Dark surface palette paired with the given accent.
    #[must_use]
    pub fn ultra_dark(accent: AccentPalette) -> Self {
        let (accent_primary, accent_selection, accent_text) = dark_accent_colors(accent);
        Self {
            dark_mode: true,
            bg_canvas: Color32::from_rgb(12, 14, 18),
            bg_panel: Color32::from_rgb(18, 21, 27),
            bg_elevated: Color32::from_rgb(27, 32, 41),
            bg_faint: Color32::from_rgb(22, 26, 33),
            bg_extreme: Color32::from_rgb(8, 10, 13),
            bg_hover: Color32::from_rgb(38, 45, 57),
            border_subtle: Color32::from_rgb(48, 56, 68),
            text_primary: Color32::from_rgb(234, 239, 247),
            text_muted: Color32::from_rgb(162, 173, 189),
            accent_primary,
            accent_selection,
            accent_text,
            status_ok: Color32::from_rgb(86, 204, 128),
            status_warn: Color32::from_rgb(235, 176, 60),
            status_err: Color32::from_rgb(240, 105, 105),
        }
    }

    /// Static Apple-style Light surface palette paired with the given accent.
    #[must_use]
    pub fn light(accent: AccentPalette) -> Self {
        let (accent_primary, accent_selection, accent_text) = light_accent_colors(accent);
        Self {
            dark_mode: false,
            bg_canvas: Color32::from_rgb(245, 245, 247),
            bg_panel: Color32::from_rgb(255, 255, 255),
            bg_elevated: Color32::from_rgb(232, 234, 239),
            bg_faint: Color32::from_rgb(238, 240, 244),
            bg_extreme: Color32::from_rgb(224, 227, 233),
            bg_hover: Color32::from_rgb(218, 222, 230),
            border_subtle: Color32::from_rgb(204, 209, 219),
            text_primary: Color32::from_rgb(20, 22, 26),
            text_muted: Color32::from_rgb(82, 89, 102),
            accent_primary,
            accent_selection,
            accent_text,
            status_ok: Color32::from_rgb(16, 122, 58),
            status_warn: Color32::from_rgb(148, 86, 0),
            status_err: Color32::from_rgb(188, 36, 46),
        }
    }

    /// Resolve a `Palette` from `ThemeMode`, OS dark state, `AccentPalette`, and optional dynamic album accent.
    #[must_use]
    pub fn resolve(
        mode: ThemeMode,
        os_dark: bool,
        accent: AccentPalette,
        dynamic_color: Option<Color32>,
    ) -> Self {
        let dark = match mode {
            ThemeMode::System => os_dark,
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
        };
        let mut pal = if dark {
            Self::ultra_dark(accent)
        } else {
            Self::light(accent)
        };
        if let Some(raw) = dynamic_color {
            let (primary, selection, text) = tune_accent_for_mode(raw, dark);
            pal.accent_primary = primary;
            pal.accent_selection = selection;
            pal.accent_text = text;
        }
        pal
    }
}

fn dark_accent_colors(accent: AccentPalette) -> (Color32, Color32, Color32) {
    match accent {
        AccentPalette::Emerald => (
            Color32::from_rgb(76, 175, 128),
            Color32::from_rgb(36, 74, 58),
            Color32::from_rgb(12, 16, 14),
        ),
        AccentPalette::Gold => (
            Color32::from_rgb(224, 178, 66),
            Color32::from_rgb(78, 60, 24),
            Color32::from_rgb(18, 14, 6),
        ),
        AccentPalette::YtmPink => (
            Color32::from_rgb(255, 118, 152),
            Color32::from_rgb(88, 32, 50),
            Color32::from_rgb(18, 10, 14),
        ),
    }
}

fn light_accent_colors(accent: AccentPalette) -> (Color32, Color32, Color32) {
    match accent {
        AccentPalette::Emerald => (
            Color32::from_rgb(20, 124, 72),
            Color32::from_rgb(208, 238, 222),
            Color32::from_rgb(255, 255, 255),
        ),
        AccentPalette::Gold => (
            Color32::from_rgb(142, 94, 4),
            Color32::from_rgb(248, 234, 198),
            Color32::from_rgb(255, 255, 255),
        ),
        AccentPalette::YtmPink => (
            Color32::from_rgb(196, 26, 72),
            Color32::from_rgb(252, 220, 230),
            Color32::from_rgb(255, 255, 255),
        ),
    }
}

/// Stateful theme controller holding the active `Palette` and smoothly animating dynamic accents.
#[derive(Debug, Clone)]
pub struct ThemeState {
    pub os_dark: bool,
    pub dynamic_target: Option<Color32>,
    pub dynamic_current: Option<Color32>,
    pub palette: Palette,
    last_applied_key: Option<(bool, AccentPalette, Option<Color32>, bool, i32)>,
}

impl Default for ThemeState {
    fn default() -> Self {
        Self {
            os_dark: true,
            dynamic_target: None,
            dynamic_current: None,
            palette: Palette::ultra_dark(AccentPalette::Emerald),
            last_applied_key: None,
        }
    }
}

/// Accessibility settings affecting theme rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccessibilityConfig {
    pub high_contrast: bool,
    pub font_scale: f32,
}

impl Default for AccessibilityConfig {
    fn default() -> Self {
        Self {
            high_contrast: false,
            font_scale: 1.0,
        }
    }
}

impl ThemeState {
    /// Update active palette and apply `egui::Visuals` only when colors change (zero steady-state churn).
    /// Returns `true` when still animating dynamic accent interpolation.
    #[allow(clippy::too_many_arguments)]
    pub fn sync_and_apply(
        &mut self,
        ctx: &egui::Context,
        mode: ThemeMode,
        accent: AccentPalette,
        dynamic_enabled: bool,
        reduced_motion: bool,
        dt: f32,
        accessibility: AccessibilityConfig,
    ) -> bool {
        let mut animating = false;
        let active_dyn = if dynamic_enabled {
            match (self.dynamic_current, self.dynamic_target) {
                (Some(cur), Some(target)) if cur != target && !reduced_motion => {
                    let step = (1.0 - (-dt.clamp(0.0, 0.1) * 8.0).exp()).clamp(0.08, 1.0);
                    let next = lerp_color(cur, target, step);
                    self.dynamic_current = Some(next);
                    animating = next != target;
                    Some(next)
                }
                (_, target) => {
                    self.dynamic_current = target;
                    target
                }
            }
        } else {
            self.dynamic_current = None;
            None
        };

        let dark = match mode {
            ThemeMode::System => self.os_dark,
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
        };
        let key = (
            dark,
            accent,
            active_dyn,
            accessibility.high_contrast,
            (accessibility.font_scale * 100.0) as i32,
        );
        if self.last_applied_key != Some(key) {
            self.palette = Palette::resolve(mode, self.os_dark, accent, active_dyn);
            apply_palette(
                ctx,
                &self.palette,
                accessibility.high_contrast,
                accessibility.font_scale,
            );
            self.last_applied_key = Some(key);
        }
        animating
    }
}

/// Apply the initial Ultra-Dark theme once at startup before the first frame.
pub fn apply_theme(ctx: &egui::Context, high_contrast: bool, font_scale: f32) {
    apply_palette(
        ctx,
        &Palette::ultra_dark(AccentPalette::Emerald),
        high_contrast,
        font_scale,
    );
}

/// Apply a resolved `Palette` to `egui::Context` visuals.
pub fn apply_palette(ctx: &egui::Context, pal: &Palette, high_contrast: bool, font_scale: f32) {
    let mut visuals = if pal.dark_mode {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.panel_fill = pal.bg_panel;
    visuals.window_fill = pal.bg_canvas;
    visuals.faint_bg_color = pal.bg_faint;
    visuals.extreme_bg_color = pal.bg_extreme;
    visuals.override_text_color = Some(pal.text_primary);
    visuals.selection.bg_fill = pal.accent_selection;
    visuals.selection.stroke = Stroke::new(1.0_f32, pal.accent_primary);

    // High contrast mode: increase contrast for better accessibility
    let (text_primary, border_subtle, accent_primary) = if high_contrast {
        if pal.dark_mode {
            (
                Color32::from_rgb(255, 255, 255),
                Color32::from_rgb(128, 128, 128),
                Color32::from_rgb(255, 255, 0),
            )
        } else {
            (
                Color32::from_rgb(0, 0, 0),
                Color32::from_rgb(64, 64, 64),
                Color32::from_rgb(0, 100, 0),
            )
        }
    } else {
        (pal.text_primary, pal.border_subtle, pal.accent_primary)
    };

    visuals.widgets.noninteractive.bg_fill = pal.bg_panel;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, border_subtle);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, text_primary);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = pal.bg_elevated;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, border_subtle);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, text_primary);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = pal.bg_hover;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, accent_primary);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, text_primary);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = pal.accent_selection;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, accent_primary);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, text_primary);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    // Focus ring for accessibility - visible focus indicator
    visuals.widgets.active.fg_stroke = Stroke::new(2.0_f32, accent_primary);

    visuals.window_stroke = Stroke::new(1.0_f32, border_subtle);
    visuals.window_corner_radius = CornerRadius::same(6);

    // Apply font scaling
    let mut style = (*ctx.style()).clone();
    style.text_styles.iter_mut().for_each(|(_, font_id)| {
        font_id.size = (font_id.size * font_scale).clamp(8.0, 48.0);
    });
    ctx.set_style(style);

    let (pref, target_theme) = if pal.dark_mode {
        (egui::ThemePreference::Dark, egui::Theme::Dark)
    } else {
        (egui::ThemePreference::Light, egui::Theme::Light)
    };
    ctx.set_theme(pref);
    ctx.set_visuals_of(target_theme, visuals);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_accent::contrast_ratio;

    #[test]
    fn all_palettes_meet_wcag_aa_contrast_on_every_surface() {
        for dark in [true, false] {
            for accent in AccentPalette::ALL {
                let pal = if dark {
                    Palette::ultra_dark(accent)
                } else {
                    Palette::light(accent)
                };
                for (name, bg) in [
                    ("canvas", pal.bg_canvas),
                    ("panel", pal.bg_panel),
                    ("elevated", pal.bg_elevated),
                ] {
                    let primary = contrast_ratio(pal.text_primary, bg);
                    assert!(
                        primary >= 4.5,
                        "primary text contrast {primary:.2} < 4.5 on {name} (dark={dark}, accent={accent:?})"
                    );
                    let muted = contrast_ratio(pal.text_muted, bg);
                    assert!(
                        muted >= 4.5,
                        "muted text contrast {muted:.2} < 4.5 on {name} (dark={dark}, accent={accent:?})"
                    );
                }
                let btn = contrast_ratio(pal.accent_text, pal.accent_primary);
                assert!(
                    btn >= 4.5,
                    "accent text contrast {btn:.2} < 4.5 (dark={dark}, accent={accent:?})"
                );
            }
        }
    }
}
