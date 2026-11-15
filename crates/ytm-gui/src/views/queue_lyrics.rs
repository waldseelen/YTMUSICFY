use eframe::egui;

use crate::app::YtmGuiApp;
use crate::navigation::ToastState;
use crate::playback_ops::{PlayAction, start_playing_track};
use crate::state::{AsyncState, WorkerCmd};
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
                if !app.play_queue.tracks().is_empty() && ui.button("Clear").clicked() {
                    app.play_queue.clear();
                    app.player.stop();
                    app.toast = Some(ToastState::info(true, "Play queue cleared."));
                }
            });
            ui.separator();

            if app.play_queue.tracks().is_empty() {
                ui.label("Queue is empty. Double-click any track in Inspector or Browse to start playing.");
            } else {
                let cur_idx = app.play_queue.current_index();
                let len = app.play_queue.tracks().len();
                egui::ScrollArea::vertical()
                    .id_salt("queue_scroll")
                    .show(ui, |ui| {
                        for (idx, t) in app.play_queue.tracks().iter().enumerate() {
                            let active = cur_idx == Some(idx);
                            ui.horizontal(|ui| {
                                let label = if active {
                                    egui::RichText::new(format!("▶ {}", idx + 1))
                                        .strong()
                                        .color(egui::Color32::from_rgb(90, 210, 130))
                                } else {
                                    egui::RichText::new(format!("{}", idx + 1))
                                };
                                if ui.small_button(label).clicked() {
                                    jump_to = Some(idx);
                                }
                                if let Some(url) = t.effective_thumbnail_url()
                                    && let Some(tex) = app.artwork.texture_for(ui.ctx(), &url)
                                {
                                    ui.add(
                                        egui::Image::new((tex.id(), egui::vec2(22.0, 22.0)))
                                            .corner_radius(3.0),
                                    );
                                }
                                if ui.selectable_label(active, &t.title).double_clicked() {
                                    jump_to = Some(idx);
                                }
                                ui.small(t.primary_artist_name().unwrap_or("-"));
                                if let Some(d) = t.duration_sec {
                                    ui.small(format_duration(u64::from(d)));
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("✕").on_hover_text("Remove").clicked() {
                                            remove_idx = Some(idx);
                                        }
                                        if idx + 1 < len && ui.small_button("↓").clicked() {
                                            move_pair = Some((idx, idx + 1));
                                        }
                                        if idx > 0 && ui.small_button("↑").clicked() {
                                            move_pair = Some((idx, idx - 1));
                                        }
                                    },
                                );
                            });
                        }
                    });
            }
        });

        cols[1].group(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Lyrics");
                if let Some(lbid) = app.lyrics_browse_id.as_ref()
                    && ui.button("↻ Reload").clicked()
                {
                    app.lyrics_state = AsyncState::Loading("Loading lyrics...".to_string());
                    let _ = app.cmd_tx.send(WorkerCmd::LoadLyrics {
                        browse_id: lbid.clone(),
                    });
                }
            });
            if let Some(cur) = app.play_queue.current_track() {
                ui.small(format!(
                    "{} — {}",
                    cur.title,
                    cur.primary_artist_name().unwrap_or("-")
                ));
            }
            ui.separator();

            match &app.lyrics_state {
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
                AsyncState::Ready(Some(text)) => {
                    egui::ScrollArea::vertical()
                        .id_salt("lyrics_scroll")
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(text.as_str()).wrap());
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
