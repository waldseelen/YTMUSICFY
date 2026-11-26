use std::sync::mpsc::Sender;

use eframe::egui;

use crate::state::{GuiAppConfig, MutationPolicy, WorkerCmd};

/// Dedicated Settings panel editing `config.toml` governance, InnerTube pacing, and enrichment options.
pub fn show(
    ui: &mut egui::Ui,
    cfg: &mut GuiAppConfig,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.heading("Application Settings (config.toml)");
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("settings_panel_scroll")
        .show(ui, |ui| {
            ui.strong("Safety & Mutation Governance");
            ui.horizontal(|ui| {
                ui.label("Mutation Policy:");
                ui.radio_value(
                    &mut cfg.mutation_policy,
                    MutationPolicy::StrictLock,
                    "StrictLock (All writes including ♥ stage in Sync)",
                );
                ui.radio_value(
                    &mut cfg.mutation_policy,
                    MutationPolicy::QuickLike,
                    "QuickLike (♥ immediate + rollback; playlists locked)",
                );
            });
            ui.checkbox(
                &mut cfg.auto_refresh_browser_cookies,
                "Auto-refresh browser cookies via yt-dlp on startup if expired (OFF by default)",
            );

            ui.add_space(10.0);
            ui.separator();
            ui.strong("InnerTube Protocol & Pacing");
            ui.horizontal(|ui| {
                ui.label("Language (hl):");
                ui.add(egui::TextEdit::singleline(&mut cfg.innertube.hl).desired_width(60.0));
                ui.label("Region (gl):");
                ui.add(egui::TextEdit::singleline(&mut cfg.innertube.gl).desired_width(60.0));
            });
            ui.horizontal(|ui| {
                ui.label("Mutation chunk size:");
                ui.add(
                    egui::DragValue::new(&mut cfg.innertube.mutation_chunk_size).range(1..=50),
                );
                ui.label("Mutation delay (ms):");
                ui.add(
                    egui::DragValue::new(&mut cfg.innertube.mutation_delay_ms).range(0..=5000),
                );
                ui.label("Max pagination pages:");
                ui.add(
                    egui::DragValue::new(&mut cfg.innertube.max_pagination_pages).range(5..=2000),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.strong("Genre Enrichment Providers");
            ui.checkbox(
                &mut cfg.enrichment.artist_cache_first,
                "Fast Mode (artist_cache_first: reuse cached artist genre before querying MusicBrainz)",
            );
            ui.checkbox(
                &mut cfg.enrichment.enable_deezer_fallback,
                "Enable Deezer fallback",
            );
            ui.checkbox(
                &mut cfg.enrichment.enable_listenbrainz_fallback,
                "Enable ListenBrainz fallback",
            );
            ui.horizontal(|ui| {
                ui.label("Last.fm API Key (optional):");
                let mut lfm = cfg.enrichment.lastfm_api_key.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut lfm)
                            .password(true)
                            .desired_width(240.0),
                    )
                    .changed()
                {
                    cfg.enrichment.lastfm_api_key =
                        (!lfm.trim().is_empty()).then(|| lfm.trim().to_string());
                }
            });

            ui.add_space(12.0);
            if ui.button("Save Settings to config.toml").clicked() {
                let _ = cmd_tx.send(WorkerCmd::UpdateGuiConfig(cfg.clone()));
            }
        });
}
