use eframe::egui;
use std::time::Duration;
use ytm_core::{PlaylistSummary, Track};

use crate::audio_fx::AudioEffects;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button};
use crate::navigation::ToastState;
use crate::play_queue::{PlayQueue, RepeatMode};
use crate::playback_ops::PlayAction;
use crate::player::{PlayerController, PlayerSnapshot, PlayerState};
use crate::right_drawer::RightDrawerTab;
use crate::sync_queue::SyncQueue;
use crate::views::inspect_helpers::format_duration;

#[path = "player_bar_left.rs"]
mod left;
#[path = "player_bar_right.rs"]
mod right;

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
    pub toggle_queue_drawer: bool,
    pub toggle_lyrics_drawer: bool,
    pub open_lightbox: bool,
    pub open_ytm_url: Option<String>,
    pub copy_ytm_url: Option<String>,
    pub apply_pending_configure: bool,
    pub stage_add: Option<(String, String, Track)>,
    pub apply_audio_filters: Option<String>,
    pub set_speed: Option<f64>,
    pub set_sleep_timer: Option<Duration>,
    pub cancel_sleep_timer: bool,
    pub actions: Vec<PlayAction>,
}

pub struct PlayerBarCtx<'a> {
    pub lang: ResolvedLang,
    pub queue: &'a PlayQueue,
    pub player: &'a PlayerController,
    pub playlists: &'a [PlaylistSummary],
    pub favourite_playlist_id: Option<&'a str>,
    pub muted: bool,
    pub right_drawer: RightDrawerTab,
    pub art_tex: Option<&'a egui::TextureHandle>,
    pub current_effects: &'a mut AudioEffects,
    pub sync_queue: &'a mut SyncQueue,
    pub toast: &'a mut Option<ToastState>,
}

/// Render responsive 3-zone persistent bottom audio player bar (`mpv` IPC + `PlayQueue`).
pub fn show(ui: &mut egui::Ui, bar: PlayerBarCtx<'_>) -> PlayerBarOutcome {
    let PlayerBarCtx {
        lang,
        queue,
        player,
        playlists,
        favourite_playlist_id,
        muted,
        right_drawer,
        art_tex,
        current_effects,
        sync_queue,
        toast,
    } = bar;
    let mut out = PlayerBarOutcome::default();
    let snap = player.snapshot();
    let current = queue.current_track();

    let available = ui.available_width();
    let font_scale = ui
        .style()
        .text_styles
        .get(&egui::TextStyle::Body)
        .map(|f| (f.size / 14.0).clamp(0.8, 2.0))
        .unwrap_or(1.0);
    let layout = crate::bar_layout::PlayerBarLayout::from_width(available, font_scale);

    if layout.compact_two_row {
        ui.vertical(|ui| {
            render_center_transport(ui, lang, queue, &snap, &mut out);
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(layout.side_width, ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                        left::render_left_track_info(
                            ui,
                            left::LeftTrackCtx {
                                lang,
                                current,
                                queue,
                                playlists,
                                art_tex,
                                sync_queue,
                                toast,
                            },
                            &mut out,
                        );
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    right::render_right_controls(
                        ui,
                        right::RightControlsCtx {
                            lang,
                            current,
                            queue,
                            snap: &snap,
                            muted,
                            right_drawer,
                            favourite_playlist_id,
                            playlists,
                            current_effects,
                            show_secondary_inline: layout.show_secondary_inline,
                            show_codec_badge: layout.show_codec_badge,
                            volume_width: layout.volume_width,
                        },
                        &mut out,
                    );
                });
            });
        });
    } else {
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(layout.side_width, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(layout.side_width);
                    ui.set_max_width(layout.side_width);
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    left::render_left_track_info(
                        ui,
                        left::LeftTrackCtx {
                            lang,
                            current,
                            queue,
                            playlists,
                            art_tex,
                            sync_queue,
                            toast,
                        },
                        &mut out,
                    );
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(layout.center_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_min_width(layout.center_width);
                    ui.set_max_width(layout.center_width);
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    render_center_transport(ui, lang, queue, &snap, &mut out);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(layout.side_width, ui.available_height()),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_min_width(layout.side_width);
                    ui.set_max_width(layout.side_width);
                    ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
                    right::render_right_controls(
                        ui,
                        right::RightControlsCtx {
                            lang,
                            current,
                            queue,
                            snap: &snap,
                            muted,
                            right_drawer,
                            favourite_playlist_id,
                            playlists,
                            current_effects,
                            show_secondary_inline: layout.show_secondary_inline,
                            show_codec_badge: layout.show_codec_badge,
                            volume_width: layout.volume_width,
                        },
                        &mut out,
                    );
                },
            );
        });
    }

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

fn render_center_transport(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    queue: &PlayQueue,
    snap: &PlayerSnapshot,
    out: &mut PlayerBarOutcome,
) {
    let has_current = queue.current_track().is_some();
    ui.vertical_centered(|ui| {
        ui.horizontal(|ui| {
            let side_pad = ((ui.available_width() - 168.0) * 0.5).max(0.0);
            if side_pad > 0.0 {
                ui.add_space(side_pad);
            }
            let shuf_tint = if queue.is_shuffle() {
                ui.visuals().selection.stroke.color
            } else {
                ui.visuals().text_color()
            };
            if ui
                .add(
                    egui::Button::image(Icon::Shuffle.image(15.0, shuf_tint))
                        .selected(queue.is_shuffle()),
                )
                .on_hover_text(tr(lang, Key::PlayerShuffle))
                .clicked()
            {
                out.toggle_shuffle = true;
            }
            if ui
                .add_enabled(
                    has_current,
                    egui::Button::image(Icon::Prev.image(18.0, ui.visuals().text_color())),
                )
                .on_hover_text(tr(lang, Key::PlayerPrev))
                .clicked()
            {
                out.previous = true;
            }
            let play_icon = match &snap.state {
                PlayerState::Playing { .. } => Icon::Pause,
                PlayerState::Loading { .. } => Icon::Refresh,
                _ => Icon::Play,
            };
            let play_hover = match &snap.state {
                PlayerState::Playing { .. } => tr(lang, Key::PlayerPause),
                PlayerState::Loading { .. } => "Loading...",
                _ => tr(lang, Key::PlayerPlay),
            };
            let is_playing = matches!(snap.state, PlayerState::Playing { .. });
            let play_btn = egui::Button::image(play_icon.image(18.0, ui.visuals().text_color()))
                .corner_radius(14.0)
                .min_size(egui::vec2(34.0, 28.0))
                .selected(is_playing);
            if ui
                .add(play_btn)
                .on_hover_text(format!("{} (Space)", play_hover))
                .clicked()
            {
                out.toggle_pause = true;
            }
            if icon_button(ui, Icon::Next, 18.0, has_current, tr(lang, Key::PlayerNext)).clicked() {
                out.next = true;
            }
            let rep = queue.repeat_mode();
            if ui
                .add(
                    egui::Button::image(rep.icon_kind().image(15.0, ui.visuals().text_color()))
                        .selected(rep != RepeatMode::Off),
                )
                .on_hover_text(tr(lang, Key::PlayerRepeat))
                .clicked()
            {
                out.cycle_repeat = true;
            }
        });

        ui.horizontal(|ui| {
            let max_dur = snap.duration_sec.max(1.0) as f32;
            let mut pos = (snap.position_sec as f32).clamp(0.0, max_dur);
            let pos_str = format_duration(snap.position_sec.max(0.0) as u64);
            let dur_str = format_duration(snap.duration_sec.max(0.0) as u64);
            ui.monospace(pos_str);
            let mono_font = egui::TextStyle::Monospace.resolve(ui.style());
            let dur_w = ui.fonts(|f| {
                f.layout_no_wrap(dur_str.clone(), mono_font, ui.visuals().text_color())
                    .size()
                    .x
            });
            let slider_w =
                (ui.available_width() - dur_w - ui.spacing().item_spacing.x - 16.0).max(40.0);
            let resp = ui
                .add_enabled_ui(has_current && snap.duration_sec > 0.0, |ui| {
                    ui.spacing_mut().slider_width = slider_w;
                    ui.add(
                        egui::Slider::new(&mut pos, 0.0..=max_dur)
                            .show_value(false)
                            .trailing_fill(true),
                    )
                })
                .inner;
            if resp.hovered() {
                if let Some(pointer) = ui.ctx().pointer_latest_pos()
                    && resp.rect.width() > 1.0
                {
                    let frac = ((pointer.x - resp.rect.left()) / resp.rect.width()).clamp(0.0, 1.0);
                    let hover_sec = (f64::from(frac) * snap.duration_sec.max(0.0)) as u64;
                    resp.clone().on_hover_text(format_duration(hover_sec));
                }
                let (scroll, shift) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
                if let Some(target) = crate::bar_layout::scroll_seek_target(
                    snap.position_sec,
                    snap.duration_sec,
                    scroll.y,
                    scroll.x,
                    shift,
                ) {
                    out.seek_to = Some(target);
                }
            }
            if resp.drag_stopped() || (resp.changed() && !resp.dragged()) {
                out.seek_to = Some(f64::from(pos));
            }
            ui.monospace(dur_str);
        });
        ui.add_space(2.0);
    });
}
