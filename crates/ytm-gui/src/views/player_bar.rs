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
    // Responsive min widths: left 180, center 280, right 160 = 620 total minimum
    // If narrower, left shrinks first, then center, right stays for controls
    let left_min = (available * 0.25).clamp(140.0, 220.0);
    let center_min = (available * 0.5).clamp(240.0, 400.0);

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(left_min, ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(left_min);
                render_left_track_info(ui, lang, current, queue, playlists, art_tex, &mut out);
            },
        );
        ui.separator();
        ui.allocate_ui_with_layout(
            egui::vec2(center_min, ui.available_height()),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(center_min);
                render_center_transport(ui, lang, queue, &snap, &mut out);
            },
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            render_right_controls(
                ui,
                RightControlsCtx {
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
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
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
                    egui::Button::image(Icon::Prev.image(15.0, ui.visuals().text_color())),
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
            if ui
                .add(
                    egui::Button::image(play_icon.image(15.0, ui.visuals().text_color()))
                        .selected(matches!(snap.state, PlayerState::Playing { .. })),
                )
                .on_hover_text(format!("{} (Space)", play_hover))
                .clicked()
            {
                out.toggle_pause = true;
            }
            if icon_button(ui, Icon::Next, 15.0, has_current, tr(lang, Key::PlayerNext)).clicked() {
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

struct RightControlsCtx<'a> {
    lang: ResolvedLang,
    current: Option<&'a Track>,
    queue: &'a PlayQueue,
    snap: &'a PlayerSnapshot,
    muted: bool,
    right_drawer: RightDrawerTab,
    favourite_playlist_id: Option<&'a str>,
    playlists: &'a [PlaylistSummary],
    current_effects: &'a mut AudioEffects,
}

fn render_right_controls(ui: &mut egui::Ui, ctx: RightControlsCtx<'_>, out: &mut PlayerBarOutcome) {
    let RightControlsCtx {
        lang,
        current,
        queue,
        snap,
        muted,
        right_drawer,
        favourite_playlist_id,
        playlists,
        current_effects,
    } = ctx;
    if ui
        .add_enabled(
            snap.state.is_active(),
            egui::Button::image(Icon::Stop.image(14.0, ui.visuals().text_color())),
        )
        .on_hover_text(tr(lang, Key::PlayerStop))
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
    let mute_icon = if muted || snap.volume <= 0.1 {
        Icon::VolumeMute
    } else {
        Icon::VolumeOn
    };
    if ui
        .add(
            egui::Button::image(mute_icon.image(14.0, ui.visuals().text_color())),
        )
        .on_hover_text(format!("{:.0}% — {}", snap.volume, tr(lang, Key::PlayerMute)))
        .clicked()
    {
        out.toggle_mute = true;
    }
    if ui
        .add(
            egui::Button::image(
                Icon::Mic.image(14.0, ui.visuals().text_color()),
            )
            .selected(right_drawer == RightDrawerTab::Lyrics),
        )
        .on_hover_text(tr(lang, Key::PlayerLyrics))
        .clicked()
    {
        out.toggle_lyrics_drawer = true;
    }
    // Quick-add to favourite playlist
    if let (Some(fav_id), Some(track)) = (favourite_playlist_id, current)
        && !fav_id.is_empty()
    {
        let fav_title = playlists
            .iter()
            .find(|p| p.id == fav_id)
            .map(|p| p.title.as_str())
            .unwrap_or("Favourite Playlist");
        if ui
            .add(
                egui::Button::image(Icon::Plus.image(14.0, ui.visuals().text_color())),
            )
            .on_hover_text(format!("{} — {}", fav_title, tr(lang, Key::PlayerQuickAddTooltip)))
            .clicked()
        {
            out.stage_add = Some((fav_id.to_string(), fav_title.to_string(), track.clone()));
        }
    }
    let q_len = queue.tracks().len();
    if ui
        .add(
            egui::Button::image(
                Icon::ListMusic.image(14.0, ui.visuals().text_color()),
            )
            .selected(right_drawer == RightDrawerTab::Queue),
        )
        .on_hover_text(format!("{} ({q_len})", tr(lang, Key::PlayerQueue)))
        .clicked()
    {
        out.toggle_queue_drawer = true;
    }
    if let Some(track) = current {
        if ui
            .add(
                egui::Button::image(Icon::Radio.image(14.0, ui.visuals().text_color())),
            )
            .on_hover_text(tr(lang, Key::PlayerRadio))
            .clicked()
        {
            out.actions.push(PlayAction::StartRadio(track.clone()));
        }
        ui.menu_button(
            "YT",
            |ui| {
            let base_url = crate::youtube_links::ytm_track_url(&track.video_id, None);
            let at_url = crate::youtube_links::ytm_track_url(
                &track.video_id,
                Some(snap.position_sec.max(0.0) as u64),
            );
            let search_url = crate::youtube_links::youtube_search_url(&format!(
                "{} {}",
                track.primary_artist_name().unwrap_or(""),
                track.title
            ));
            if ui.button(tr(lang, Key::OpenInYtm)).clicked() {
                out.open_ytm_url = Some(base_url.clone());
                ui.close_menu();
            }
            if ui.button(tr(lang, Key::OpenInYtmAtTime)).clicked() {
                out.open_ytm_url = Some(at_url);
                ui.close_menu();
            }
            if ui.button(tr(lang, Key::SearchOnYoutube)).clicked() {
                out.open_ytm_url = Some(search_url);
                ui.close_menu();
            }
            if ui.button(tr(lang, Key::CopyLink)).clicked() {
                out.copy_ytm_url = Some(base_url);
                ui.close_menu();
            }
        });
    }
    // Audio Effects menu
    ui.menu_button(
        "FX",
        |ui| {
        ui.strong(tr(lang, Key::PlayerEqPreset));
        for preset in crate::audio_fx::EqPreset::ALL {
            let selected = current_effects.eq_preset == preset;
            if ui.selectable_label(selected, preset.label()).clicked() {
                current_effects.eq_preset = preset;
                out.apply_audio_filters = Some(preset.filter_string().to_string());
                ui.close_menu();
            }
        }
        ui.separator();
        ui.strong(tr(lang, Key::PlayerSpeed));
        let mut speed_val = current_effects.speed.0;
        let speed_text = format!("{:.2}x", speed_val as f64 / 100.0);
        if ui
            .add(
                egui::Slider::new(&mut speed_val, 75..=150)
                    .text(speed_text)
                    .integer(),
            )
            .changed()
        {
            current_effects.speed = crate::audio_fx::PlaybackSpeed(speed_val);
            out.set_speed = Some(current_effects.speed.as_f64());
        }
        ui.separator();
        ui.strong(tr(lang, Key::PlayerSleepTimer));
        if current_effects.sleep_timer.is_active() {
            if let Some(remaining) = current_effects.sleep_timer.remaining_str() {
                ui.label(format!("Remaining: {remaining}"));
            }
            if ui.button("Cancel").clicked() {
                let _ = current_effects.sleep_timer.cancel();
                out.cancel_sleep_timer = true;
                ui.close_menu();
            }
        } else {
            for &mins in &[15, 30, 45, 60, 90, 120] {
                if ui.button(format!("{mins} min")).clicked() {
                    current_effects.sleep_timer =
                        crate::audio_fx::SleepTimerState::new(Duration::from_secs(mins * 60));
                    out.set_sleep_timer = Some(Duration::from_secs(mins * 60));
                    ui.close_menu();
                }
            }
        }
    });
    if let Some(codec) = &snap.codec_name {
        let badge = match snap.bitrate_kbps {
            Some(kbps) if kbps > 0 => format!("{} {kbps}k", codec.to_ascii_uppercase()),
            _ => codec.to_ascii_uppercase(),
        };
        ui.colored_label(egui::Color32::from_rgb(115, 200, 150), badge)
            .on_hover_text("Live mpv audio stream codec & bitrate");
    }
}
