use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::Playlist;
use ytm_store::ExportFormat;

use crate::state::{AsyncState, WorkerCmd};
use crate::views::async_banner;

pub fn show(
    ui: &mut egui::Ui,
    playlist_state: &mut AsyncState<Playlist>,
    search: &mut String,
    export_dir: &mut String,
    export_fmt: &mut ExportFormat,
    cmd_tx: &Sender<WorkerCmd>,
) {
    let (loading, error) = match playlist_state {
        AsyncState::Loading(m) => (Some(m.as_str()), None),
        AsyncState::Failed(e) => (None, Some(e.as_str())),
        _ => (None, None),
    };
    async_banner(ui, loading, error);

    let Some(pl) = playlist_state.data() else {
        ui.label("Select a playlist from the left sidebar to inspect tracks.");
        return;
    };

    ui.horizontal(|ui| {
        ui.heading(&pl.title);
        ui.label(format!(
            "ID: {} | Tracks: {} | Owned: {}",
            pl.id,
            pl.tracks.len(),
            if pl.owned { "yes" } else { "no" }
        ));
    });

    ui.horizontal(|ui| {
        ui.label("Filter:");
        ui.text_edit_singleline(search);
        ui.separator();
        egui::ComboBox::from_id_salt("export_fmt_combo")
            .selected_text(format!("{export_fmt:?}"))
            .show_ui(ui, |ui| {
                ui.selectable_value(export_fmt, ExportFormat::Json, "JSON");
                ui.selectable_value(export_fmt, ExportFormat::Csv, "CSV");
                ui.selectable_value(export_fmt, ExportFormat::M3u8, "M3U8");
            });
        ui.text_edit_singleline(export_dir);
        if ui.button("Backup & Export").clicked() {
            let _ = cmd_tx.send(WorkerCmd::BackupPlaylist {
                target: pl.id.clone(),
                format: *export_fmt,
                out_dir: PathBuf::from(export_dir.trim()),
            });
        }
    });
    ui.separator();

    let needle = search.trim().to_ascii_lowercase();
    let filtered: Vec<_> = pl
        .tracks
        .iter()
        .filter(|t| {
            needle.is_empty()
                || t.title.to_ascii_lowercase().contains(&needle)
                || t.primary_artist_name()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .contains(&needle)
                || t.video_id.to_ascii_lowercase().contains(&needle)
        })
        .collect();

    TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .column(Column::exact(38.0))
        .column(Column::initial(220.0).clip(true))
        .column(Column::initial(160.0).clip(true))
        .column(Column::initial(65.0))
        .column(Column::exact(50.0))
        .column(Column::remainder())
        .header(22.0, |mut header| {
            for h in ["#", "Title", "Artist", "Dur(s)", "Type", "Status"] {
                header.col(|ui| {
                    ui.strong(h);
                });
            }
        })
        .body(|body| {
            body.rows(20.0, filtered.len(), |mut row| {
                let t = filtered[row.index()];
                row.col(|ui| {
                    ui.label((t.position + 1).to_string());
                });
                row.col(|ui| {
                    ui.label(&t.title);
                });
                row.col(|ui| {
                    ui.label(t.primary_artist_name().unwrap_or("-"));
                });
                row.col(|ui| {
                    ui.label(
                        t.duration_sec
                            .map_or_else(|| "-".to_string(), |s| s.to_string()),
                    );
                });
                row.col(|ui| {
                    ui.label(t.music_video_type.short_label());
                });
                row.col(|ui| {
                    let status = if t.availability.is_available() {
                        "OK".to_string()
                    } else {
                        format!("{:?}", t.availability)
                    };
                    ui.label(status);
                });
            });
        });
}
