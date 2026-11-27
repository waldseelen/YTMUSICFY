use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};
use ytm_store::ExportFormat;

use crate::state::{GuiTab, WorkerCmd};

/// Explicit keyboard focus owner preventing sidebar vs main-table Arrow/Enter collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusPane {
    #[default]
    MainTable,
    Sidebar,
    Queue,
    Drawer,
    Modal,
}

/// Fuzzy `Ctrl+K` Command Palette jumping to any workspace, playlist, or core action.
#[allow(clippy::too_many_arguments)]
pub fn show_command_palette(
    ctx: &egui::Context,
    open: &mut bool,
    query: &mut String,
    playlists: &[PlaylistSummary],
    active_tab: &mut GuiTab,
    selected_playlist_id: &mut Option<String>,
    sync_drawer_open: &mut bool,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &HashMap<String, Playlist>,
    export_fmt: ExportFormat,
    export_dir: &str,
    current_track: Option<(&Track, u64)>,
    preferred_browser: Option<&str>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    if !*open {
        return;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = false;
        query.clear();
        return;
    }

    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("cmd_palette_backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let resp = ui.allocate_response(screen.size(), egui::Sense::click());
            ui.painter()
                .rect_filled(screen, 0.0, egui::Color32::from_black_alpha(110));
            if resp.clicked() {
                *open = false;
                query.clear();
            }
        });

    let needle = query.trim().to_ascii_lowercase();
    let mut close_now = false;

    egui::Window::new("Command Palette (Ctrl+K)")
        .order(egui::Order::Foreground)
        .collapsible(false)
        .resizable(false)
        .fixed_size([460.0, 340.0])
        .anchor(egui::Align2::CENTER_TOP, [0.0, 72.0])
        .show(ctx, |ui| {
            let edit = ui.add(
                egui::TextEdit::singleline(query)
                    .desired_width(f32::INFINITY)
                    .hint_text("Type a playlist, workspace, or action..."),
            );
            edit.request_focus();
            ui.separator();

            egui::ScrollArea::vertical()
                .id_salt("cmd_palette_scroll")
                .show(ui, |ui| {
                    let commands: [(&str, GuiTab); 11] = [
                        ("Workspace: Home & Explore (Browse)", GuiTab::Browse),
                        ("Workspace: Catalog Search", GuiTab::Search),
                        ("Workspace: Queue & Lyrics", GuiTab::QueueLyrics),
                        ("Workspace: Library & Playlist Inspector", GuiTab::Inspector),
                        ("Studio Tool: Dedupe", GuiTab::Dedupe),
                        ("Studio Tool: Sort (LIS)", GuiTab::Sort),
                        ("Studio Tool: Set Algebra", GuiTab::SetOps),
                        ("Studio Tool: Health Scanner", GuiTab::Health),
                        ("Studio Tool: Genre Split", GuiTab::GenreSplit),
                        ("Safety & History: Snapshots", GuiTab::Snapshots),
                        ("Settings & Auth: Application Settings", GuiTab::Settings),
                    ];

                    for (label, tab) in commands {
                        if (needle.is_empty() || label.to_ascii_lowercase().contains(&needle))
                            && ui.selectable_label(false, label).clicked()
                        {
                            *active_tab = tab;
                            close_now = true;
                        }
                    }

                    if (needle.is_empty() || "sync drawer review dispatch".contains(&needle))
                        && ui
                            .selectable_label(
                                false,
                                "Action: Toggle Sync Staging Drawer (Ctrl+Shift+S)",
                            )
                            .clicked()
                    {
                        *sync_drawer_open = !*sync_drawer_open;
                        close_now = true;
                    }
                    if (needle.is_empty() || "settings auth config".contains(&needle))
                        && ui
                            .selectable_label(false, "Workspace: Settings & Auth")
                            .clicked()
                    {
                        *active_tab = GuiTab::Auth;
                        close_now = true;
                    }
                    if (needle.is_empty() || "refresh library reload".contains(&needle))
                        && ui
                            .selectable_label(false, "Action: Refresh YouTube Music Library")
                            .clicked()
                    {
                        let _ = cmd_tx.send(WorkerCmd::RefreshLibrary);
                        close_now = true;
                    }
                    if (needle.is_empty()
                        || "backup all export playlists snapshots".contains(&needle))
                        && ui
                            .selectable_label(false, "Action: Backup All Playlists")
                            .clicked()
                    {
                        let _ = cmd_tx.send(WorkerCmd::BackupPlaylist {
                            target: "all".to_string(),
                            format: export_fmt,
                            out_dir: PathBuf::from(export_dir.trim()),
                        });
                        close_now = true;
                    }
                    if let Some((track, pos_sec)) = current_track {
                        if (needle.is_empty() || "open youtube music ctrl+o".contains(&needle))
                            && ui
                                .selectable_label(false, "Action: Open in YouTube Music (Ctrl+O)")
                                .clicked()
                        {
                            let url = crate::youtube_links::ytm_track_url(&track.video_id, None);
                            let _ = crate::browser_launch::open_url_in_browser(
                                &url,
                                preferred_browser,
                            );
                            close_now = true;
                        }
                        if (needle.is_empty()
                            || "open youtube music current second timestamp ctrl+shift+o"
                                .contains(&needle))
                            && ui
                                .selectable_label(
                                    false,
                                    "Action: Open in YouTube Music at current second (Ctrl+Shift+O)",
                                )
                                .clicked()
                        {
                            let url = crate::youtube_links::ytm_track_url(
                                &track.video_id,
                                Some(pos_sec),
                            );
                            let _ = crate::browser_launch::open_url_in_browser(
                                &url,
                                preferred_browser,
                            );
                            close_now = true;
                        }
                        if (needle.is_empty() || "search on youtube".contains(&needle))
                            && ui
                                .selectable_label(false, "Action: Search Current Track on YouTube")
                                .clicked()
                        {
                            let q = format!(
                                "{} {}",
                                track.primary_artist_name().unwrap_or(""),
                                track.title
                            );
                            let url = crate::youtube_links::youtube_search_url(&q);
                            let _ = crate::browser_launch::open_url_in_browser(
                                &url,
                                preferred_browser,
                            );
                            close_now = true;
                        }
                        if (needle.is_empty() || "copy link url".contains(&needle))
                            && ui
                                .selectable_label(false, "Action: Copy YouTube Music Link")
                                .clicked()
                        {
                            let url = crate::youtube_links::ytm_track_url(&track.video_id, None);
                            ctx.copy_text(url);
                            close_now = true;
                        }
                    }

                    if !playlists.is_empty() {
                        ui.separator();
                        ui.weak("Playlists");
                        for pl in playlists.iter().filter(|p| {
                            needle.is_empty() || p.title.to_ascii_lowercase().contains(&needle)
                        }) {
                            if ui
                                .selectable_label(
                                    false,
                                    format!("Playlist: {} ({})", pl.title, pl.id),
                                )
                                .clicked()
                            {
                                *selected_playlist_id = Some(pl.id.clone());
                                *active_tab = GuiTab::Inspector;
                                if !playlist_cache.contains_key(&pl.id)
                                    && loading_playlists.insert(pl.id.clone())
                                {
                                    let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                                }
                                close_now = true;
                            }
                        }
                    }
                });
        });

    if close_now {
        *open = false;
        query.clear();
    }
}
