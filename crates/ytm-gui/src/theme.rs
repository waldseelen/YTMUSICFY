use eframe::egui::{self, Color32, CornerRadius, Stroke, Visuals};

/// Semantic surface & accent palette for `ytm-architect` desktop studio.
pub const BG_CANVAS: Color32 = Color32::from_rgb(17, 19, 23);
pub const BG_PANEL: Color32 = Color32::from_rgb(23, 27, 33);
pub const BG_ELEVATED: Color32 = Color32::from_rgb(31, 37, 46);
pub const BORDER_SUBTLE: Color32 = Color32::from_rgb(48, 56, 68);
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(230, 236, 245);
pub const ACCENT_PRIMARY: Color32 = Color32::from_rgb(76, 168, 122);
pub const ACCENT_SELECTION: Color32 = Color32::from_rgb(42, 74, 62);

/// Apply the cohesive dark studio `egui::Visuals` theme once at startup.
pub fn apply_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG_PANEL;
    visuals.window_fill = BG_CANVAS;
    visuals.faint_bg_color = Color32::from_rgb(26, 31, 38);
    visuals.extreme_bg_color = Color32::from_rgb(13, 15, 19);
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.selection.bg_fill = ACCENT_SELECTION;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_PRIMARY);

    visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(4);

    visuals.widgets.inactive.bg_fill = BG_ELEVATED;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(4);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(40, 48, 59);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_PRIMARY);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(4);

    visuals.widgets.active.bg_fill = ACCENT_SELECTION;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_PRIMARY);
    visuals.widgets.active.corner_radius = CornerRadius::same(4);

    visuals.window_stroke = Stroke::new(1.0, BORDER_SUBTLE);
    visuals.window_corner_radius = CornerRadius::same(6);

    ctx.set_visuals(visuals);
}
