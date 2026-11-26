use eframe::egui;
use ytm_core::{PlaylistAccess, PlaylistSummary, Track};

use crate::icons;
use crate::play_queue::PlayQueue;
use crate::playback_ops::PlayAction;
use crate::player::{PlayerController, PlayerSnapshot, PlayerState};
use crate::views::inspect_helpers::format_duration;

/// Control events emitted by the persistent bottom player bar.
#[derive(Default)]
pub struct PlayerBarOutcome {
    pub toggle_pause: bool,
    pub previous: bool,
    pub next: bool,
    pub stop: bool,
    pub toggle_shuffle: bool,
    pub cycle_repeat: bool,
    pub toggle_mute: bool,
    pub seek_to: Option<f64>,
    pub volume_to: Option<f64>,
    pub open_queue_lyrics: bool,
    pub apply_pending_configure: bool,
    pub stage_add: Option<(String, String, Track)>,
    pub actions: Vec<PlayAction>,
}

/// Render responsive 3-zone persistent bottom audio player bar (`mpv` IPC + `PlayQueue`).
pub fn show(
    ui: &mut egui::Ui,
    queue: &PlayQueue,
    player: &PlayerController,
    playlists: &[PlaylistSummary],
    muted: bool,
    art_tex: Option<&egui::TextureHandle>,
) -> PlayerBarOutcome {
    let mut out = PlayerBarOutcome::default();
    let snap = player.snapshot();
    let current = queue.current_track();

    ui.horizontal(|ui| {
        render_left_track_info(ui, current, queue, playlists, art_tex, &mut out);
        ui.separator();
        render_center_transport(ui, queue, &snap, &mut out);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            render_right_controls(ui, current, queue, &snap, muted, &mut out);
        });
    });

    if let PlayerState::Error(err) = &snap.state {
        ui.colored_label(
            egui::Color32::from_rgb(235, 100, 100),
            format!("Player error: {err}"),
        );
    }
    if let Some(pq) = snap.pending_configure
        && snap.state.is_active()
    {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(235, 185, 65),
                format!(
                    "Pending stream config ({}) — applies on next track.",
                    pq.label()
                ),
            );
            if ui.small_button("Apply Now").clicked() {
                out.apply_pending_configure = true;
            }
        });
    }

    out
}

fn render_left_track_info(
    ui: &mut egui::Ui,
    current: Option<&Track>,
    queue: &PlayQueue,
    playlists: &[PlaylistSummary],
    art_tex: Option<&egui::TextureHandle>,
    out: &mut PlayerBarOutcome,
) {
    let art_size = egui::vec2(42.0, 42.0);
    if let Some(tex) = art_tex {
        ui.add(egui::Image::new((tex.id(), art_size)).corner_radius(4.0));
    } else {
        let (rect, _) = ui.allocate_exact_size(art_size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 4.0, egui::Color32::from_gray(38));
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "♪",
            egui::FontId::proportional(18.0),
            egui::Color32::from_gray(160),
        );
    }

    let Some(track) = current else {
        ui.vertical(|ui| {
            ui.strong("Not playing");
            ui.small("Double-click any track or press Play");
        });
        return;
    };

    ui.vertical(|ui| {
        ui.set_max_width(220.0);
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(egui::RichText::new(&track.title).strong()).truncate());
            let liked = queue.is_liked(&track.video_id);
            let heart = if liked {
                icons::HEART_ON
            } else {
                icons::HEART_OFF
            };
            let color = if liked {
                egui::Color32::from_rgb(235, 85, 115)
            } else {
                egui::Color32::GRAY
            };
            if ui
                .button(egui::RichText::new(heart).color(color))
                .on_hover_text(if liked { "Remove Like" } else { "Like Track" })
                .clicked()
            {
                out.actions.push(PlayAction::ToggleLike(track.clone()));
            }
            ui.menu_button("+", |ui| {
                ui.label("Stage add to playlist:");
                for pl in playlists
                    .iter()
                    .filter(|p| p.access() == PlaylistAccess::EditableOwned)
                {
                    if ui.button(&pl.title).clicked() {
                        out.stage_add = Some((pl.id.clone(), pl.title.clone(), track.clone()));
                        ui.close_menu();
                    }
                }
            });
        });
        ui.horizontal(|ui| {
            if let Some(artist) = track.artists.first() {
                if let Some(bid) = &artist.browse_id {
                    if ui.link(&artist.name).clicked() {
                        out.actions.push(PlayAction::OpenBrowse(bid.clone()));
                    }
                } else {
                    ui.small(&artist.name);
                }
            }
            if let Some(album) = &track.album {
                ui.small("•");
                if let Some(bid) = &album.browse_id {
                    if ui.link(&album.name).clicked() {
                        out.actions.push(PlayAction::OpenBrowse(bid.clone()));
                    }
                } else {
                    ui.small(&album.name);
                }
            }
        });
    });
}

fn render_center_transport(
    ui: &mut egui::Ui,
    queue: &PlayQueue,
    snap: &PlayerSnapshot,
    out: &mut PlayerBarOutcome,
) {
    let has_current = queue.current_track().is_some();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if ui
                .selectable_label(queue.is_shuffle(), icons::SHUFFLE)
                .on_hover_text("Toggle shuffle")
                .clicked()
            {
                out.toggle_shuffle = true;
            }
            if ui
                .add_enabled(has_current, egui::Button::new(icons::PREV))
                .on_hover_text("Previous track")
                .clicked()
            {
                out.previous = true;
            }
            let play_icon = match &snap.state {
                PlayerState::Playing { .. } => format!("{} Pause", icons::PAUSE),
                PlayerState::Loading { .. } => "Loading...".to_string(),
                _ => format!("{} Play", icons::PLAY),
            };
            if ui
                .add_enabled(has_current, egui::Button::new(play_icon))
                .on_hover_text("Play / Pause (Space)")
                .clicked()
            {
                out.toggle_pause = true;
            }
            if ui
                .add_enabled(has_current, egui::Button::new(icons::NEXT))
                .on_hover_text("Next track")
                .clicked()
            {
                out.next = true;
            }
            if ui
                .button(queue.repeat_mode().icon())
                .on_hover_text("Cycle repeat: Off → All → One")
                .clicked()
            {
                out.cycle_repeat = true;
            }
        });

        ui.horizontal(|ui| {
            let max_dur = snap.duration_sec.max(1.0) as f32;
            let mut pos = (snap.position_sec as f32).clamp(0.0, max_dur);
            ui.monospace(format_duration(snap.position_sec.max(0.0) as u64));
            let resp = ui.add_enabled(
                has_current && snap.duration_sec > 0.0,
                egui::Slider::new(&mut pos, 0.0..=max_dur)
                    .show_value(false)
                    .trailing_fill(true),
            );
            if resp.hovered()
                && let Some(pointer) = ui.ctx().pointer_latest_pos()
                && resp.rect.width() > 1.0
            {
                let frac = ((pointer.x - resp.rect.left()) / resp.rect.width()).clamp(0.0, 1.0);
                let hover_sec = (f64::from(frac) * snap.duration_sec.max(0.0)) as u64;
                resp.clone().on_hover_text(format_duration(hover_sec));
            }
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                out.seek_to = Some(f64::from(pos));
            }
            ui.monospace(format_duration(snap.duration_sec.max(0.0) as u64));
        });
    });
}

fn render_right_controls(
    ui: &mut egui::Ui,
    current: Option<&Track>,
    queue: &PlayQueue,
    snap: &PlayerSnapshot,
    muted: bool,
    out: &mut PlayerBarOutcome,
) {
    if ui
        .add_enabled(snap.state.is_active(), egui::Button::new(icons::STOP))
        .on_hover_text("Stop playback")
        .clicked()
    {
        out.stop = true;
    }
    let mut vol = snap.volume as f32;
    let slider = egui::Slider::new(&mut vol, 0.0..=100.0)
        .show_value(false)
        .trailing_fill(true);
    if ui.add_sized([74.0, 18.0], slider).changed() {
        out.volume_to = Some(f64::from(vol));
    }
    let mute_label = if muted || snap.volume <= 0.1 {
        icons::MUTE_ON
    } else {
        icons::MUTE_OFF
    };
    if ui
        .small_button(format!("{mute_label} {:.0}%", snap.volume))
        .on_hover_text("Toggle mute (M)")
        .clicked()
    {
        out.toggle_mute = true;
    }
    let q_len = queue.tracks().len();
    if ui
        .button(format!("Queue ({q_len})"))
        .on_hover_text("Open Play Queue and Lyrics")
        .clicked()
    {
        out.open_queue_lyrics = true;
    }
    if let Some(track) = current
        && ui
            .button("Radio")
            .on_hover_text("Seed Radio station from current track")
            .clicked()
    {
        out.actions.push(PlayAction::StartRadio(track.clone()));
    }
    if let Some(codec) = &snap.codec_name {
        let badge = match snap.bitrate_kbps {
            Some(kbps) if kbps > 0 => format!("{} {kbps}k", codec.to_ascii_uppercase()),
            _ => codec.to_ascii_uppercase(),
        };
        ui.colored_label(egui::Color32::from_rgb(115, 200, 150), badge)
            .on_hover_text("Live mpv audio stream codec & bitrate");
    }
}
