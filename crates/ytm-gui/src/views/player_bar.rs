use eframe::egui;
use ytm_core::{PlaylistAccess, PlaylistSummary, Track};

use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, icon_text_button};
use crate::play_queue::PlayQueue;
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
    pub actions: Vec<PlayAction>,
}

pub struct PlayerBarCtx<'a> {
    pub lang: ResolvedLang,
    pub queue: &'a PlayQueue,
    pub player: &'a PlayerController,
    pub playlists: &'a [PlaylistSummary],
    pub muted: bool,
    pub right_drawer: RightDrawerTab,
    pub art_tex: Option<&'a egui::TextureHandle>,
}

/// Render responsive 3-zone persistent bottom audio player bar (`mpv` IPC + `PlayQueue`).
pub fn show(ui: &mut egui::Ui, bar: PlayerBarCtx<'_>) -> PlayerBarOutcome {
    let PlayerBarCtx {
        lang,
        queue,
        player,
        playlists,
        muted,
        right_drawer,
        art_tex,
    } = bar;
    let mut out = PlayerBarOutcome::default();
    let snap = player.snapshot();
    let current = queue.current_track();

    ui.horizontal(|ui| {
        render_left_track_info(ui, lang, current, queue, playlists, art_tex, &mut out);
        ui.separator();
        render_center_transport(ui, lang, queue, &snap, &mut out);
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
            if icon_button(ui, Icon::Prev, 15.0, has_current, tr(lang, Key::PlayerPrev)).clicked() {
                out.previous = true;
            }
            let (play_icon, play_label) = match &snap.state {
                PlayerState::Playing { .. } => (Icon::Pause, tr(lang, Key::PlayerPause)),
                PlayerState::Loading { .. } => (Icon::Refresh, "..."),
                _ => (Icon::Play, tr(lang, Key::PlayerPlay)),
            };
            if icon_text_button(ui, play_icon, play_label, 15.0, has_current)
                .on_hover_text("Play / Pause (Space)")
                .clicked()
            {
                out.toggle_pause = true;
            }
            if icon_button(ui, Icon::Next, 15.0, has_current, tr(lang, Key::PlayerNext)).clicked() {
                out.next = true;
            }
            let rep = queue.repeat_mode();
            if icon_text_button(ui, rep.icon_kind(), rep.short_label(), 15.0, true)
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
}

fn render_right_controls(ui: &mut egui::Ui, ctx: RightControlsCtx<'_>, out: &mut PlayerBarOutcome) {
    let RightControlsCtx {
        lang,
        current,
        queue,
        snap,
        muted,
        right_drawer,
    } = ctx;
    if icon_button(
        ui,
        Icon::Stop,
        14.0,
        snap.state.is_active(),
        tr(lang, Key::PlayerStop),
    )
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
        .add(egui::Button::image_and_text(
            mute_icon.image(14.0, ui.visuals().text_color()),
            format!("{:.0}%", snap.volume),
        ))
        .on_hover_text(tr(lang, Key::PlayerMute))
        .clicked()
    {
        out.toggle_mute = true;
    }
    if ui
        .add(
            egui::Button::image_and_text(
                Icon::Mic.image(14.0, ui.visuals().text_color()),
                tr(lang, Key::PlayerLyrics),
            )
            .selected(right_drawer == RightDrawerTab::Lyrics),
        )
        .clicked()
    {
        out.toggle_lyrics_drawer = true;
    }
    let q_len = queue.tracks().len();
    if ui
        .add(
            egui::Button::image_and_text(
                Icon::ListMusic.image(14.0, ui.visuals().text_color()),
                format!("{} ({q_len})", tr(lang, Key::PlayerQueue)),
            )
            .selected(right_drawer == RightDrawerTab::Queue),
        )
        .clicked()
    {
        out.toggle_queue_drawer = true;
    }
    if let Some(track) = current
        && icon_text_button(ui, Icon::Radio, tr(lang, Key::PlayerRadio), 14.0, true).clicked()
    {
        out.actions.push(PlayAction::StartRadio(track.clone()));
    }
    if let Some(track) = current {
        ui.menu_button("YT", |ui| {
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
    if let Some(codec) = &snap.codec_name {
        let badge = match snap.bitrate_kbps {
            Some(kbps) if kbps > 0 => format!("{} {kbps}k", codec.to_ascii_uppercase()),
            _ => codec.to_ascii_uppercase(),
        };
        ui.colored_label(egui::Color32::from_rgb(115, 200, 150), badge)
            .on_hover_text("Live mpv audio stream codec & bitrate");
    }
}
