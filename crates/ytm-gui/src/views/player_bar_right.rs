use std::time::Duration;

use eframe::egui;
use ytm_core::{PlaylistSummary, Track};

use crate::audio_fx::AudioEffects;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::Icon;
use crate::play_queue::PlayQueue;
use crate::playback_ops::PlayAction;
use crate::player::PlayerSnapshot;
use crate::right_drawer::RightDrawerTab;
use crate::views::player_bar::PlayerBarOutcome;

pub(super) struct RightControlsCtx<'a> {
    pub lang: ResolvedLang,
    pub current: Option<&'a Track>,
    pub queue: &'a PlayQueue,
    pub snap: &'a PlayerSnapshot,
    pub muted: bool,
    pub right_drawer: RightDrawerTab,
    pub favourite_playlist_id: Option<&'a str>,
    pub playlists: &'a [PlaylistSummary],
    pub current_effects: &'a mut AudioEffects,
    pub show_secondary_inline: bool,
    pub show_codec_badge: bool,
    pub volume_width: f32,
}

fn format_codec_badge(snap: &PlayerSnapshot) -> Option<String> {
    snap.codec_name
        .as_ref()
        .map(|codec| match snap.bitrate_kbps {
            Some(kbps) if kbps > 0 => format!("{} {kbps}k", codec.to_ascii_uppercase()),
            _ => codec.to_ascii_uppercase(),
        })
}

pub(super) fn render_right_controls(
    ui: &mut egui::Ui,
    ctx: RightControlsCtx<'_>,
    out: &mut PlayerBarOutcome,
) {
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
        show_secondary_inline,
        show_codec_badge,
        volume_width,
    } = ctx;
    ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
    ui.spacing_mut().item_spacing.x = 4.0;
    ui.spacing_mut().interact_size = egui::vec2(26.0, 22.0);
    let mut vol = snap.volume as f32;
    let vol_slider_resp = if volume_width >= 36.0 {
        let slider = egui::Slider::new(&mut vol, 0.0..=100.0)
            .show_value(false)
            .trailing_fill(true);
        let resp = ui.add_sized([volume_width, 18.0], slider);
        if resp.changed() {
            out.volume_to = Some(f64::from(vol));
        }
        Some(resp)
    } else {
        None
    };
    let mute_icon = if muted || snap.volume <= 0.1 {
        Icon::VolumeMute
    } else {
        Icon::VolumeOn
    };
    let codec_hint = snap
        .codec_name
        .as_ref()
        .map(|codec| match snap.bitrate_kbps {
            Some(kbps) if kbps > 0 => format!(" · {} {kbps}k", codec.to_ascii_uppercase()),
            _ => format!(" · {}", codec.to_ascii_uppercase()),
        })
        .unwrap_or_default();
    let mute_resp = ui
        .add(egui::Button::image(
            mute_icon.image(14.0, ui.visuals().text_color()),
        ))
        .on_hover_text(format!(
            "{:.0}% — {}{codec_hint}",
            snap.volume,
            tr(lang, Key::PlayerMute)
        ));
    if mute_resp.clicked() {
        out.toggle_mute = true;
    }
    if vol_slider_resp.as_ref().is_some_and(|r| r.hovered()) || mute_resp.hovered() {
        let (scroll, shift) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
        if let Some(target) =
            crate::bar_layout::scroll_volume_target(snap.volume, scroll.y, scroll.x, shift)
        {
            out.volume_to = Some(target);
        }
    }
    if ui
        .add(
            egui::Button::image(Icon::Mic.image(14.0, ui.visuals().text_color()))
                .selected(right_drawer == RightDrawerTab::Lyrics),
        )
        .on_hover_text(tr(lang, Key::PlayerLyrics))
        .clicked()
    {
        out.toggle_lyrics_drawer = true;
    }
    if show_secondary_inline
        && let (Some(fav_id), Some(track)) = (favourite_playlist_id, current)
        && !fav_id.is_empty()
    {
        let fav_title = playlists
            .iter()
            .find(|p| p.id == fav_id)
            .map(|p| p.title.as_str())
            .unwrap_or("Favourite Playlist");
        if ui
            .add(egui::Button::image(
                Icon::Plus.image(14.0, ui.visuals().text_color()),
            ))
            .on_hover_text(format!(
                "{} — {}",
                fav_title,
                tr(lang, Key::PlayerQuickAddTooltip)
            ))
            .clicked()
        {
            out.stage_add = Some((fav_id.to_string(), fav_title.to_string(), track.clone()));
        }
    }
    let q_len = queue.tracks().len();
    if ui
        .add(
            egui::Button::image(Icon::ListMusic.image(14.0, ui.visuals().text_color()))
                .selected(right_drawer == RightDrawerTab::Queue),
        )
        .on_hover_text(format!("{} ({q_len})", tr(lang, Key::PlayerQueue)))
        .clicked()
    {
        out.toggle_queue_drawer = true;
    }
    if show_secondary_inline {
        if let Some(track) = current {
            if ui
                .add(egui::Button::image(
                    Icon::Radio.image(14.0, ui.visuals().text_color()),
                ))
                .on_hover_text(tr(lang, Key::PlayerRadio))
                .clicked()
            {
                out.actions.push(PlayAction::StartRadio(track.clone()));
            }
            ui.menu_button("YT", |ui| render_yt_menu(ui, lang, track, snap, out));
        }
        ui.menu_button("FX", |ui| render_fx_menu(ui, lang, current_effects, out));
        if show_codec_badge && let Some(badge) = format_codec_badge(snap) {
            ui.colored_label(egui::Color32::from_rgb(115, 200, 150), badge)
                .on_hover_text("Live mpv audio stream codec & bitrate");
        }
    } else {
        ui.menu_button("•••", |ui| {
            if let Some(badge) = format_codec_badge(snap) {
                ui.colored_label(
                    egui::Color32::from_rgb(115, 200, 150),
                    format!("Stream: {badge}"),
                );
                ui.separator();
            }
            if let Some(track) = current {
                if ui.button(tr(lang, Key::PlayerRadio)).clicked() {
                    out.actions.push(PlayAction::StartRadio(track.clone()));
                    ui.close_menu();
                }
                if let Some(fav_id) = favourite_playlist_id
                    && !fav_id.is_empty()
                {
                    let fav_title = playlists
                        .iter()
                        .find(|p| p.id == fav_id)
                        .map(|p| p.title.as_str())
                        .unwrap_or("Favourite Playlist");
                    if ui.button(format!("+ {fav_title}")).clicked() {
                        out.stage_add =
                            Some((fav_id.to_string(), fav_title.to_string(), track.clone()));
                        ui.close_menu();
                    }
                }
                ui.menu_button("YouTube Links", |ui| {
                    render_yt_menu(ui, lang, track, snap, out)
                });
                ui.separator();
            }
            ui.menu_button("Audio FX / EQ / Timer", |ui| {
                render_fx_menu(ui, lang, current_effects, out);
            });
        })
        .response
        .on_hover_text("Radio, YouTube links, Audio FX & Codec info");
    }
}

fn render_yt_menu(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    track: &Track,
    snap: &PlayerSnapshot,
    out: &mut PlayerBarOutcome,
) {
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
}

fn render_fx_menu(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    current_effects: &mut AudioEffects,
    out: &mut PlayerBarOutcome,
) {
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
}
