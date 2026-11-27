use eframe::egui;
use ytm_core::{PlaylistAccess, PlaylistSummary, Track};

use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_text_button};
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::sync_queue::{StagedOp, SyncQueue};
pub use crate::views::context_menu_model::ContextTarget;
use crate::views::context_menu_model::{ContextActionId, build_menu_spec};

/// Shared environment required to render any context menu across Dalga views.
pub struct ContextMenuEnv<'a> {
    pub lang: ResolvedLang,
    pub playlists: &'a [PlaylistSummary],
    pub sync_queue: &'a mut SyncQueue,
    pub toast: &'a mut Option<ToastState>,
    pub actions: &'a mut Vec<PlayAction>,
}

/// Optional Inspector-specific playlist editing extras appended to track context menus.
pub struct InspectorContextExtras<'a> {
    pub pl_id: &'a str,
    pub editable: bool,
    pub selection_tracks: &'a [Track],
    pub batch_copy_move: &'a mut Option<(String, String, Vec<Track>, bool)>,
    pub remove_single: &'a mut Option<Track>,
    pub filter_artist: &'a mut Option<String>,
}

/// Attach the single shared context menu builder to `resp` for any `ContextTarget`.
pub fn attach_context_menu(
    resp: &egui::Response,
    target: ContextTarget,
    env: ContextMenuEnv<'_>,
    mut extras: Option<InspectorContextExtras<'_>>,
) {
    let has_editable = env
        .playlists
        .iter()
        .any(|p| p.access() == PlaylistAccess::EditableOwned);
    let specs = build_menu_spec(&target, has_editable);
    let lang = env.lang;
    let playlists = env.playlists;
    let sync_queue = env.sync_queue;
    let toast = env.toast;
    let actions = env.actions;

    resp.context_menu(|ui| {
        for spec in &specs {
            match spec.id {
                ContextActionId::PlayNow => {
                    if let ContextTarget::Track {
                        queue_list,
                        row_idx,
                        ..
                    } = &target
                        && icon_text_button(ui, Icon::Play, tr(lang, Key::CtxPlayNow), 13.0, true)
                            .clicked()
                    {
                        actions.push(PlayAction::PlayFromList {
                            tracks: queue_list.clone(),
                            start_idx: *row_idx,
                        });
                        ui.close_menu();
                    }
                }
                ContextActionId::PlayNext => {
                    if let ContextTarget::Track { track, .. } = &target
                        && icon_text_button(ui, Icon::Next, tr(lang, Key::CtxPlayNext), 13.0, true)
                            .clicked()
                    {
                        actions.push(PlayAction::PlayNext(track.clone()));
                        ui.close_menu();
                    }
                }
                ContextActionId::AddQueue => {
                    if let ContextTarget::Track { track, .. } = &target
                        && icon_text_button(ui, Icon::Plus, tr(lang, Key::CtxAddQueue), 13.0, true)
                            .clicked()
                    {
                        actions.push(PlayAction::AppendQueue(track.clone()));
                        ui.close_menu();
                    }
                }
                ContextActionId::StartRadio => {
                    if let ContextTarget::Track { track, .. } = &target
                        && icon_text_button(
                            ui,
                            Icon::Radio,
                            tr(lang, Key::CtxStartRadio),
                            13.0,
                            true,
                        )
                        .clicked()
                    {
                        actions.push(PlayAction::StartRadio(track.clone()));
                        ui.close_menu();
                    }
                }
                ContextActionId::ToggleLike => {
                    if let ContextTarget::Track { track, .. } = &target
                        && icon_text_button(
                            ui,
                            Icon::Heart,
                            tr(lang, Key::CtxToggleLike),
                            13.0,
                            true,
                        )
                        .clicked()
                    {
                        actions.push(PlayAction::ToggleLike(track.clone()));
                        ui.close_menu();
                    }
                }
                ContextActionId::AddToPlaylist => {
                    if let ContextTarget::Track { track, .. } = &target {
                        ui.menu_button(tr(lang, Key::CtxStageAdd), |ui| {
                            for dest in playlists
                                .iter()
                                .filter(|p| p.access() == PlaylistAccess::EditableOwned)
                            {
                                if ui.button(&dest.title).clicked() {
                                    let eid = sync_queue.push(StagedOp::AddTracks {
                                        playlist_id: dest.id.clone(),
                                        playlist_title: dest.title.clone(),
                                        tracks: vec![track.clone()],
                                        allow_duplicates: false,
                                    });
                                    *toast = Some(ToastState::staged(
                                        format!("Staged '{}' -> '{}'", track.title, dest.title),
                                        Some(eid),
                                    ));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                }
                ContextActionId::OpenBrowse => {
                    let bid = match &target {
                        ContextTarget::Album { browse_id, .. }
                        | ContextTarget::Artist { browse_id, .. } => Some(browse_id.clone()),
                        ContextTarget::Playlist { browse_id, .. } => browse_id.clone(),
                        ContextTarget::Track { .. } => None,
                    };
                    if let Some(bid) = bid
                        && icon_text_button(
                            ui,
                            Icon::ExternalLink,
                            tr(lang, Key::OpenInYtm)
                                .split_whitespace()
                                .next()
                                .unwrap_or("Open"),
                            13.0,
                            true,
                        )
                        .clicked()
                    {
                        actions.push(PlayAction::OpenBrowse(bid));
                        ui.close_menu();
                    }
                }
                ContextActionId::GoToArtist => {
                    if let Some((name, bid)) = target.artist_browse()
                        && ui
                            .button(format!("{}: {name}", tr(lang, Key::CtxGoToArtist)))
                            .clicked()
                    {
                        actions.push(PlayAction::OpenBrowse(bid));
                        ui.close_menu();
                    }
                }
                ContextActionId::GoToAlbum => {
                    if let Some((name, bid)) = target.album_browse()
                        && ui
                            .button(format!("{}: {name}", tr(lang, Key::CtxGoToAlbum)))
                            .clicked()
                    {
                        actions.push(PlayAction::OpenBrowse(bid));
                        ui.close_menu();
                    }
                }
                ContextActionId::AllOfArtist => {
                    ui.add_enabled(false, egui::Button::new(tr(lang, Key::CtxAllOfArtist)))
                        .on_disabled_hover_text(tr(lang, Key::CtxAllOfArtistSoon));
                }
                ContextActionId::OpenInYtm => {
                    if ui.button(tr(lang, Key::OpenInYtm)).clicked() {
                        let _ = crate::browser_launch::open_url_in_browser(&target.ytm_url(), None);
                        ui.close_menu();
                    }
                }
                ContextActionId::SearchOnYoutube => {
                    if ui.button(tr(lang, Key::SearchOnYoutube)).clicked() {
                        let _ = crate::browser_launch::open_url_in_browser(
                            &target.yt_search_url(),
                            None,
                        );
                        ui.close_menu();
                    }
                }
                ContextActionId::CopyLink => {
                    if icon_text_button(ui, Icon::Copy, tr(lang, Key::CopyLink), 13.0, true)
                        .clicked()
                    {
                        ui.ctx().copy_text(target.ytm_url());
                        *toast = Some(ToastState::info(true, tr(lang, Key::LinkCopiedToast)));
                        ui.close_menu();
                    }
                }
            }
        }

        if let Some(extra) = extras.as_mut()
            && let ContextTarget::Track { track, .. } = &target
        {
            render_inspector_extras(ui, track, playlists, extra);
        }
    });
}

fn render_inspector_extras(
    ui: &mut egui::Ui,
    track: &Track,
    playlists: &[PlaylistSummary],
    extra: &mut InspectorContextExtras<'_>,
) {
    ui.separator();
    if let Some(artist) = track.artists.first()
        && ui
            .button(format!("Filter by Artist: {}", artist.name))
            .clicked()
    {
        *extra.filter_artist = Some(artist.name.clone());
        ui.close_menu();
    }
    let batch_cnt = extra.selection_tracks.len();
    ui.menu_button(format!("Copy ({batch_cnt}) to Playlist..."), |ui| {
        for dest in playlists {
            if !dest.owned || dest.id.eq_ignore_ascii_case("LM") || dest.id == extra.pl_id {
                continue;
            }
            if ui.button(&dest.title).clicked() {
                *extra.batch_copy_move = Some((
                    dest.id.clone(),
                    dest.title.clone(),
                    extra.selection_tracks.to_vec(),
                    false,
                ));
                ui.close_menu();
            }
        }
    });
    ui.add_enabled_ui(extra.editable, |ui| {
        ui.menu_button(format!("Move ({batch_cnt}) to Playlist..."), |ui| {
            for dest in playlists {
                if !dest.owned || dest.id.eq_ignore_ascii_case("LM") || dest.id == extra.pl_id {
                    continue;
                }
                if ui.button(&dest.title).clicked() {
                    *extra.batch_copy_move = Some((
                        dest.id.clone(),
                        dest.title.clone(),
                        extra.selection_tracks.to_vec(),
                        true,
                    ));
                    ui.close_menu();
                }
            }
        });
    });
    if ui
        .add_enabled(extra.editable, egui::Button::new("Remove Track (Stage)"))
        .clicked()
    {
        *extra.remove_single = Some(track.clone());
        ui.close_menu();
    }
}
