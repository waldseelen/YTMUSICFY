pub mod auth;
pub mod dedupe;
pub mod genres_snapshots;
pub mod health;
pub mod inspect;
pub mod inspect_helpers;
pub mod set_ops;
pub mod sort;
pub mod split_diff;
pub mod sync;

use eframe::egui;

pub fn async_banner(ui: &mut egui::Ui, loading: Option<&str>, error: Option<&str>) {
    if let Some(msg) = loading {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(msg);
        });
    }
    if let Some(err) = error {
        ui.colored_label(
            egui::Color32::from_rgb(235, 95, 95),
            format!("Error: {err}"),
        );
    }
}
