use eframe::egui;
use std::time::Duration;
use ytm_core::{PlaylistAccess, PlaylistSummary, Track};

use crate::audio_fx::AudioEffects;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button};
use crate::play_queue::{PlayQueue, RepeatMode};
use crate::playback_ops::PlayAction;
use crate::player::{PlayerController, PlayerSnapshot, PlayerState};
use crate::right_drawer::RightDrawerTab;
use crate::views::inspect_helpers::format_duration;

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
                        render_left_track_info(
                            ui, lang, current, queue, playlists, art_tex, &mut out,
                        );
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
                    ui.set_max_width(layout.side_width);
                    render_left_track_info(ui, lang, current, queue, playlists, art_tex, &mut out);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(layout.center_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.set_max_width(layout.center_width);
                    render_center_transport(ui, lang, queue, &snap, &mut out);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(layout.side_width, ui.available_height()),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
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

fn render_left_track_info(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    current: Option<&Track>,
    queue: &PlayQueue,
    playlists: &[PlaylistSummary],
    art_tex: Option<&egui::TextureHandle>,
    out: &mut PlayerBarOutcome,
) {
    let art_size = egui::vec2(42.0, 42.0);
    if let Some(tex) = art_tex {
        let resp = ui.add(
            egui::Image::new((tex.id(), art_size))
                .corner_radius(4.0)
                .sense(egui::Sense::click()),
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text(tr(lang, Key::PlayerOpenCover)).clicked() && current.is_some() {
            out.open_lightbox = true;
        }
    } else {
        let (rect, _) = ui.allocate_exact_size(art_size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 4.0, ui.visuals().widgets.inactive.bg_fill);
        let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(20.0, 20.0));
        Icon::Music
            .image(20.0, ui.visuals().weak_text_color())
            .paint_at(ui, icon_rect);
    }

    let Some(track) = current else {
        ui.vertical(|ui| {
            ui.strong(tr(lang, Key::PlayerNotPlaying));
            ui.small(tr(lang, Key::PlayerPlayHint));
        });
        return;
    };

    ui.vertical(|ui| {
        ui.set_max_width(220.0);
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(egui::RichText::new(&track.title).strong()).truncate());
            let liked = queue.is_liked(&track.video_id);
            let color = if liked {
                egui::Color32::from_rgb(235, 85, 115)
            } else {
                ui.visuals().weak_text_color()
            };
            if ui
                .add(egui::Button::image(Icon::Heart.image(14.0, color)))
                .on_hover_text(if liked {
                    tr(lang, Key::PlayerUnlike)
                } else {
                    tr(lang, Key::PlayerLike)
                })
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
            ui.monospace(format_duration(snap.position_sec.max(0.0) as u64));
            let slider_w = (ui.available_width() - 56.0).max(80.0);
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
            ui.monospace(format_duration(snap.duration_sec.max(0.0) as u64));
        });
    });
}
