use eframe::egui;
use ytm_core::{PlaylistAccess, PlaylistSummary, Track};

use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::Icon;
use crate::navigation::ToastState;
use crate::play_queue::PlayQueue;
use crate::playback_ops::PlayAction;
use crate::sync_queue::SyncQueue;
use crate::views::context_menu::{ContextMenuEnv, ContextTarget, attach_context_menu};
use crate::views::player_bar::PlayerBarOutcome;

pub(super) struct LeftTrackCtx<'a> {
    pub lang: ResolvedLang,
    pub current: Option<&'a Track>,
    pub queue: &'a PlayQueue,
    pub playlists: &'a [PlaylistSummary],
    pub art_tex: Option<&'a egui::TextureHandle>,
    pub sync_queue: &'a mut SyncQueue,
    pub toast: &'a mut Option<ToastState>,
}

pub(super) fn render_left_track_info(
    ui: &mut egui::Ui,
    ctx: LeftTrackCtx<'_>,
    out: &mut PlayerBarOutcome,
) {
    let LeftTrackCtx {
        lang,
        current,
        queue,
        playlists,
        art_tex,
        sync_queue,
        toast,
    } = ctx;
    let art_size = egui::vec2(42.0, 42.0);
    let art_resp = if let Some(tex) = art_tex {
        let uv = crate::artwork_hires::cover_crop_uv(tex.size_vec2(), art_size);
        let resp = ui.add(
            egui::Image::new((tex.id(), art_size))
                .uv(uv)
                .corner_radius(6.0)
                .sense(egui::Sense::click()),
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp
            .clone()
            .on_hover_text(tr(lang, Key::PlayerOpenCover))
            .clicked()
            && current.is_some()
        {
            out.open_lightbox = true;
        }
        Some(resp)
    } else {
        let (rect, resp) = ui.allocate_exact_size(art_size, egui::Sense::click());
        ui.painter()
            .rect_filled(rect, 6.0, ui.visuals().widgets.inactive.bg_fill);
        let icon_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(20.0, 20.0));
        Icon::Music
            .image(20.0, ui.visuals().weak_text_color())
            .paint_at(ui, icon_rect);
        Some(resp)
    };

    let Some(track) = current else {
        ui.vertical(|ui| {
            ui.strong(tr(lang, Key::PlayerNotPlaying));
            ui.small(tr(lang, Key::PlayerPlayHint));
        });
        return;
    };

    if let Some(resp) = art_resp {
        attach_player_track_menu(&resp, track, queue, lang, playlists, sync_queue, toast, out);
    }

    let info_w = (ui.available_width() - 8.0).max(100.0);
    let info_resp = ui
        .vertical(|ui| {
            ui.set_max_width(info_w);
            ui.horizontal(|ui| {
                let title_resp = ui.add(
                    egui::Label::new(egui::RichText::new(&track.title).strong())
                        .truncate()
                        .sense(egui::Sense::click()),
                );
                attach_player_track_menu(
                    &title_resp,
                    track,
                    queue,
                    lang,
                    playlists,
                    sync_queue,
                    toast,
                    out,
                );
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
        })
        .response
        .interact(egui::Sense::click());
    attach_player_track_menu(
        &info_resp, track, queue, lang, playlists, sync_queue, toast, out,
    );
}

#[allow(clippy::too_many_arguments)]
fn attach_player_track_menu(
    resp: &egui::Response,
    track: &Track,
    queue: &PlayQueue,
    lang: ResolvedLang,
    playlists: &[PlaylistSummary],
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    out: &mut PlayerBarOutcome,
) {
    let idx = queue.current_index().unwrap_or(0);
    attach_context_menu(
        resp,
        ContextTarget::Track {
            track: track.clone(),
            queue_list: queue.tracks().to_vec(),
            row_idx: idx,
        },
        ContextMenuEnv {
            lang,
            playlists,
            sync_queue,
            toast,
            actions: &mut out.actions,
        },
        None,
    );
}
