pub mod artist_harvest;
pub mod auth;
pub mod browse;
pub mod components;
pub mod context_menu;
pub mod context_menu_model;
pub mod dedupe;
pub mod genres_snapshots;
pub mod health;
pub mod home_cards;
pub mod inspect;
pub mod inspect_actions;
pub mod inspect_helpers;
pub mod inspect_rows;
pub mod inspect_sort;
pub mod inspect_toolbar;
pub mod mini_player_search;
pub mod mini_player_view;
pub mod player_bar;
pub mod queue_lyrics;
pub mod repertoire_batch;
pub mod repertoire_target;
pub mod repertoire_tree;
pub mod search;
pub mod search_dropdown;
pub mod set_ops;
pub mod settings;
pub mod sort;
pub mod split_diff;
pub mod split_pane;
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
