use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

use crate::icons::{Icon, icon_text_button};
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::views::inspect_rows::RowRenderOutcome;

/// Context passed into each row's right-click context menu.
pub struct RowMenuContext<'a> {
    pub pl: &'a Playlist,
    pub track: &'a Track,
    pub row_idx: usize,
    pub visible: &'a [&'a Track],
    pub selection_tracks: &'a [Track],
    pub editable: bool,
    pub playlists: &'a [PlaylistSummary],
}

/// Attach playback, browse, queue, clipboard, and playlist-mutation actions to `resp`.
pub fn attach_row_context_menu(
    resp: &egui::Response,
    ctx: RowMenuContext<'_>,
    out: &mut RowRenderOutcome,
    toast: &mut Option<ToastState>,
) {
    resp.context_menu(|ui| {
        if icon_text_button(ui, Icon::Play, "Play Now", 13.0, true).clicked() {
            out.play_actions.push(PlayAction::PlayFromList {
                tracks: ctx.visible.iter().copied().cloned().collect(),
                start_idx: ctx.row_idx,
            });
            ui.close_menu();
        }
        if icon_text_button(ui, Icon::Next, "Play Next", 13.0, true).clicked() {
            out.play_actions
                .push(PlayAction::PlayNext(ctx.track.clone()));
            ui.close_menu();
        }
        if icon_text_button(ui, Icon::Plus, "Add to Queue", 13.0, true).clicked() {
            out.play_actions
                .push(PlayAction::AppendQueue(ctx.track.clone()));
            ui.close_menu();
        }
        if icon_text_button(ui, Icon::Radio, "Start Radio", 13.0, true).clicked() {
            out.play_actions
                .push(PlayAction::StartRadio(ctx.track.clone()));
            ui.close_menu();
        }
        if icon_text_button(ui, Icon::Heart, "Toggle Like", 13.0, true).clicked() {
            out.play_actions
                .push(PlayAction::ToggleLike(ctx.track.clone()));
            ui.close_menu();
        }
        ui.separator();

        if let Some(artist) = ctx.track.artists.first() {
            if let Some(bid) = &artist.browse_id
                && ui
                    .button(format!("Go to Artist: {}", artist.name))
                    .clicked()
            {
                out.play_actions.push(PlayAction::OpenBrowse(bid.clone()));
                ui.close_menu();
            }
            if ui
                .button(format!("Filter by Artist: {}", artist.name))
                .clicked()
            {
                out.filter_artist = Some(artist.name.clone());
                ui.close_menu();
            }
        }
        if let Some(album) = &ctx.track.album
            && let Some(bid) = &album.browse_id
            && ui.button(format!("Go to Album: {}", album.name)).clicked()
        {
            out.play_actions.push(PlayAction::OpenBrowse(bid.clone()));
            ui.close_menu();
        }

        let url = crate::youtube_links::ytm_track_url(&ctx.track.video_id, None);
        let search_q = format!(
            "{} {}",
            ctx.track.primary_artist_name().unwrap_or(""),
            ctx.track.title
        );
        let yt_search = crate::youtube_links::youtube_search_url(&search_q);
        if ui.button("Open in YouTube Music").clicked() {
            let _ = crate::browser_launch::open_url_in_browser(&url, None);
            ui.close_menu();
        }
        if ui.button("Search on YouTube").clicked() {
            let _ = crate::browser_launch::open_url_in_browser(&yt_search, None);
            ui.close_menu();
        }
        if ui.button("Copy link").clicked() {
            ui.ctx().copy_text(url.clone());
            *toast = Some(ToastState::info(
                true,
                "Copied YouTube Music link to clipboard.",
            ));
            ui.close_menu();
        }

        ui.separator();
        let batch_cnt = ctx.selection_tracks.len();
        ui.menu_button(format!("Copy ({batch_cnt}) to Playlist..."), |ui| {
            for dest in ctx.playlists {
                if !dest.owned || dest.id.eq_ignore_ascii_case("LM") || dest.id == ctx.pl.id {
                    continue;
                }
                if ui.button(&dest.title).clicked() {
                    out.batch_copy_move = Some((
                        dest.id.clone(),
                        dest.title.clone(),
                        ctx.selection_tracks.to_vec(),
                        false,
                    ));
                    ui.close_menu();
                }
            }
        });
        ui.add_enabled_ui(ctx.editable, |ui| {
            ui.menu_button(format!("Move ({batch_cnt}) to Playlist..."), |ui| {
                for dest in ctx.playlists {
                    if !dest.owned || dest.id.eq_ignore_ascii_case("LM") || dest.id == ctx.pl.id {
                        continue;
                    }
                    if ui.button(&dest.title).clicked() {
                        out.batch_copy_move = Some((
                            dest.id.clone(),
                            dest.title.clone(),
                            ctx.selection_tracks.to_vec(),
                            true,
                        ));
                        ui.close_menu();
                    }
                }
            });
        });
        if ui
            .add_enabled(ctx.editable, egui::Button::new("Remove Track (Stage)"))
            .clicked()
        {
            out.remove_single = Some(ctx.track.clone());
            ui.close_menu();
        }
    });
}
