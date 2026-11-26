use eframe::egui;
use egui_extras::{Column, TableBuilder};
use ytm_core::{LyricsPayload, PlaylistPrivacy};

use crate::app::YtmGuiApp;
use crate::navigation::ToastState;
use crate::playback_ops::{PlayAction, start_playing_track};
use crate::state::{AsyncState, WorkerCmd};
use crate::sync_queue::StagedOp;
use crate::views::async_banner;
use crate::views::inspect_helpers::format_duration;

/// Render live Play Queue management alongside InnerTube Lyrics panel.
pub fn show(ui: &mut egui::Ui, app: &mut YtmGuiApp) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let mut jump_to: Option<usize> = None;
    let mut move_pair: Option<(usize, usize)> = None;
    let mut remove_idx: Option<usize> = None;

    ui.columns(2, |cols| {
        cols[0].group(|ui| {
            ui.set_clip_rect(ui.max_rect());
            ui.horizontal_wrapped(|ui| {
                ui.heading(format!("Play Queue ({})", app.play_queue.tracks().len()));
                if ui
                    .selectable_label(app.play_queue.is_shuffle(), "🔀 Shuffle")
                    .clicked()
                {
                    let seed = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(7, |d| d.as_secs());
                    app.play_queue.toggle_shuffle(seed);
                }
                if ui.button(app.play_queue.repeat_mode().icon()).clicked() {
                    app.play_queue.cycle_repeat();
                }
                if let Some(cur) = app.play_queue.current_track().cloned()
                    && ui.button("📻 Start Radio").clicked()
                {
                    actions.push(PlayAction::StartRadio(cur));
                }
                if !app.play_queue.tracks().is_empty()
                    && ui
                        .button("+ Stage as Playlist")
                        .on_hover_text("Stage current play queue as a new private playlist in Sync")
                        .clicked()
                {
                    let tracks = app.play_queue.tracks().to_vec();
                    let title = format!("Queue Mix ({})", tracks.len());
                    let eid = app.sync_queue.push(StagedOp::CreatePlaylist {
                        title: title.clone(),
                        description: "Saved from ytm-gui Play Queue".to_string(),
                        privacy: PlaylistPrivacy::Private,
                        tracks,
                    });
                    app.toast = Some(ToastState::staged(
                        format!("Staged '{title}' in Sync."),
                        Some(eid),
                    ));
                }
                if !app.play_queue.tracks().is_empty() && ui.button("Clear").clicked() {
                    app.play_queue.clear();
                    app.player.stop();
                    app.toast = Some(ToastState::info(true, "Play queue cleared."));
                }
            });
            ui.separator();

            if app.play_queue.tracks().is_empty() {
                ui.label(
                    "Queue is empty. Double-click any track in Inspector or Browse to start playing.",
                );
            } else {
                render_queue_table(ui, app, &mut jump_to, &mut move_pair, &mut remove_idx);
            }
        });

        cols[1].group(|ui| {
            ui.set_clip_rect(ui.max_rect());
            ui.horizontal_wrapped(|ui| {
                ui.heading("Lyrics");
                if let Some(lbid) = app.lyrics_browse_id.as_ref()
                    && let Some(cur) = app.play_queue.current_track()
                    && ui.button("↻ Load / Reload").clicked()
                {
                    app.lyrics_payload = AsyncState::Loading("Loading lyrics...".to_string());
                    let _ = app.cmd_tx.send(WorkerCmd::LoadLyricsPayload {
                        video_id: cur.video_id.clone(),
                        browse_id: lbid.clone(),
                    });
                }
            });
            // Lazy-fetch lyrics only while Lyrics panel is visible and uncorrelated with current track.
            if let Some(lbid) = app.lyrics_browse_id.clone()
                && let Some(cur) = app.play_queue.current_track()
                && app.lyrics_video_id.as_deref() != Some(cur.video_id.as_str())
                && !app.lyrics_payload.is_loading()
            {
                app.lyrics_video_id = Some(cur.video_id.clone());
                app.lyrics_payload = AsyncState::Loading("Loading lyrics...".to_string());
                let _ = app.cmd_tx.send(WorkerCmd::LoadLyricsPayload {
                    video_id: cur.video_id.clone(),
                    browse_id: lbid,
                });
            }
            if let Some(cur) = app.play_queue.current_track() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "{} — {}",
                            cur.title,
                            cur.primary_artist_name().unwrap_or("-")
                        ))
                        .small(),
                    )
                    .truncate(),
                );
            }
            ui.separator();

            let pos_ms = (app.player.snapshot().position_sec.max(0.0) * 1000.0) as u64;
            match &app.lyrics_payload {
                AsyncState::Idle => {
                    if app.play_queue.current_track().is_none() {
                        ui.label("Play a track to fetch lyrics.");
                    } else if app.lyrics_browse_id.is_none() {
                        ui.label("No lyrics tab reported for current track.");
                    }
                }
                AsyncState::Loading(msg) => {
                    async_banner(ui, Some(msg.as_str()), None);
                }
                AsyncState::Failed(err) => {
                    async_banner(ui, None, Some(err.as_str()));
                }
                AsyncState::Ready(None) => {
                    ui.label("Lyrics unavailable for this track.");
                }
                AsyncState::Ready(Some(LyricsPayload::Plain(text))) => {
                    egui::ScrollArea::vertical()
                        .id_salt("lyrics_scroll")
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(text.as_str()).wrap());
                        });
                }
                AsyncState::Ready(Some(LyricsPayload::Synced(lines))) => {
                    egui::ScrollArea::vertical()
                        .id_salt("lyrics_synced_scroll")
                        .show(ui, |ui| {
                            for line in lines {
                                let active = pos_ms >= line.start_ms && pos_ms <= line.end_ms;
                                let rt = if active {
                                    egui::RichText::new(&line.text)
                                        .strong()
                                        .color(egui::Color32::from_rgb(90, 210, 130))
                                } else {
                                    egui::RichText::new(&line.text)
                                        .color(egui::Color32::from_gray(175))
                                };
                                ui.add(egui::Label::new(rt).wrap());
                            }
                        });
                }
            }
        });
    });

    if let Some((from, to)) = move_pair {
        app.play_queue.move_item(from, to);
    }
    if let Some(idx) = remove_idx {
        app.play_queue.remove(idx);
    }
    if let Some(idx) = jump_to
        && let Some(track) = app.play_queue.jump_to(idx).cloned()
    {
        start_playing_track(&track, app);
    }

    actions
}

fn render_queue_table(
    ui: &mut egui::Ui,
    app: &mut YtmGuiApp,
    jump_to: &mut Option<usize>,
    move_pair: &mut Option<(usize, usize)>,
    remove_idx: &mut Option<usize>,
) {
    let cur_idx = app.play_queue.current_index();
    let len = app.play_queue.tracks().len();
    let tracks = app.play_queue.tracks().to_vec();
    let table_h = ui.available_height().max(120.0);

    ui.allocate_ui(egui::vec2(ui.available_width(), table_h), |ui| {
        TableBuilder::new(ui)
            .id_salt("queue_table")
            .striped(true)
            .column(Column::exact(38.0))
            .column(Column::exact(24.0))
            .column(Column::remainder().clip(true))
            .column(Column::exact(44.0))
            .column(Column::exact(74.0))
            .body(|body| {
                body.rows(26.0, tracks.len(), |mut row| {
                    let idx = row.index();
                    let t = &tracks[idx];
                    let active = cur_idx == Some(idx);
                    row.set_selected(active);

                    row.col(|ui| {
                        let label = if active {
                            egui::RichText::new(format!("▶{}", idx + 1))
                                .strong()
                                .color(egui::Color32::from_rgb(90, 210, 130))
                        } else {
                            egui::RichText::new(format!("{}", idx + 1))
                        };
                        if ui.small_button(label).clicked() {
                            *jump_to = Some(idx);
                        }
                    });

                    row.col(|ui| {
                        if let Some(url) = t.effective_thumbnail_url()
                            && let Some(tex) = app.artwork.texture_for(ui.ctx(), &url)
                        {
                            ui.add(
                                egui::Image::new((tex.id(), egui::vec2(20.0, 20.0)))
                                    .corner_radius(3.0),
                            );
                        }
                    });

                    row.col(|ui| {
                        let text =
                            format!("{} — {}", t.title, t.primary_artist_name().unwrap_or("-"));
                        let resp = ui.add(
                            egui::Label::new(if active {
                                egui::RichText::new(text).strong()
                            } else {
                                egui::RichText::new(text)
                            })
                            .truncate()
                            .sense(egui::Sense::click()),
                        );
                        if resp.double_clicked() {
                            *jump_to = Some(idx);
                        }
                    });

                    row.col(|ui| {
                        if let Some(d) = t.duration_sec {
                            ui.small(format_duration(u64::from(d)));
                        }
                    });

                    row.col(|ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.0;
                            if ui
                                .add_enabled(idx > 0, egui::Button::new("^").small())
                                .on_hover_text("Move Up")
                                .clicked()
                            {
                                *move_pair = Some((idx, idx - 1));
                            }
                            if ui
                                .add_enabled(idx + 1 < len, egui::Button::new("v").small())
                                .on_hover_text("Move Down")
                                .clicked()
                            {
                                *move_pair = Some((idx, idx + 1));
                            }
                            if ui
                                .small_button("x")
                                .on_hover_text("Remove from queue")
                                .clicked()
                            {
                                *remove_idx = Some(idx);
                            }
                        });
                    });
                });
            });
    });
}
