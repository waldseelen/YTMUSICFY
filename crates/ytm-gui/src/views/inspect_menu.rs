use eframe::egui;
use ytm_core::{Playlist, PlaylistSummary, Track};

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
        if ui.button("▶ Play Now").clicked() {
            out.play_actions.push(PlayAction::PlayFromList {
                tracks: ctx.visible.iter().copied().cloned().collect(),
                start_idx: ctx.row_idx,
            });
            ui.close_menu();
        }
        if ui.button("⏭ Play Next").clicked() {
            out.play_actions
                .push(PlayAction::PlayNext(ctx.track.clone()));
            ui.close_menu();
        }
        if ui.button("+ Add to Queue").clicked() {
            out.play_actions
                .push(PlayAction::AppendQueue(ctx.track.clone()));
            ui.close_menu();
        }
        if ui.button("📻 Start Radio").clicked() {
            out.play_actions
                .push(PlayAction::StartRadio(ctx.track.clone()));
            ui.close_menu();
        }
        if ui.button("♥ Toggle Like").clicked() {
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

        let url = format!("https://music.youtube.com/watch?v={}", ctx.track.video_id);
        if ui.button("Copy YTM Watch URL").clicked() {
            ui.ctx().copy_text(url.clone());
            *toast = Some(ToastState::info(true, "Copied track URL to clipboard."));
            ui.close_menu();
        }
        if ui.button("Open in Browser").clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
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
