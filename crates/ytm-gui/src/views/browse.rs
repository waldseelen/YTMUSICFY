use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{
    BROWSE_EXPLORE, BROWSE_HISTORY, BROWSE_HOME, BROWSE_LIKED_ALBUMS, BrowsePage, BrowsePageKind,
    PlaylistAccess, PlaylistSummary, Track,
};

use crate::artwork::ArtworkCache;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, icon_text_button};
use crate::navigation::ToastState;
use crate::play_queue::PlayQueue;
use crate::playback_ops::PlayAction;
use crate::state::{AsyncState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;
use crate::views::context_menu::{ContextMenuEnv, ContextTarget, attach_context_menu};
use crate::views::home_cards::{
    ShelfUiEnv, paint_art_box, render_continue_card, render_shelf_card, render_skeleton_shelves,
};
use crate::views::inspect_helpers::format_duration;

pub struct BrowseCtx<'a> {
    pub lang: ResolvedLang,
    pub browse_target: &'a mut String,
    pub browse_state: &'a AsyncState<BrowsePage>,
    pub browse_from_cache: bool,
    pub play_queue: &'a PlayQueue,
    pub playlists: &'a [PlaylistSummary],
    pub sync_queue: &'a mut SyncQueue,
    pub artwork: &'a mut ArtworkCache,
    pub toast: &'a mut Option<ToastState>,
    pub cmd_tx: &'a Sender<WorkerCmd>,
}

/// Render Home (`FEmusic_home`), Explore, History, Artist, Album, or Browse shelf view.
pub fn show(ui: &mut egui::Ui, ctx: BrowseCtx<'_>) -> Vec<PlayAction> {
    let BrowseCtx {
        lang,
        browse_target,
        browse_state,
        browse_from_cache,
        play_queue,
        playlists,
        sync_queue,
        artwork,
        toast,
        cmd_tx,
    } = ctx;
    let mut actions = Vec::new();
    let mut env = ShelfUiEnv {
        lang,
        playlists,
        sync_queue,
        artwork,
        toast,
        actions: &mut actions,
    };

    ui.horizontal_wrapped(|ui| {
        if ui.button("Home").clicked() {
            env.actions
                .push(PlayAction::OpenBrowse(BROWSE_HOME.to_string()));
        }
        if ui.button("Explore").clicked() {
            env.actions
                .push(PlayAction::OpenBrowse(BROWSE_EXPLORE.to_string()));
        }
        if ui.button("History").clicked() {
            env.actions
                .push(PlayAction::OpenBrowse(BROWSE_HISTORY.to_string()));
        }
        if ui.button("Library Albums").clicked() {
            env.actions
                .push(PlayAction::OpenBrowse(BROWSE_LIKED_ALBUMS.to_string()));
        }
        ui.separator();
        ui.label("Browse ID:");
        let resp = ui.add(
            egui::TextEdit::singleline(browse_target)
                .desired_width(180.0)
                .hint_text("FEmusic_home / UC... / MPREb_..."),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Go").clicked() || enter) && !browse_target.trim().is_empty() {
            env.actions
                .push(PlayAction::OpenBrowse(browse_target.trim().to_string()));
        }
        if let AsyncState::Ready(page) = browse_state
            && icon_text_button(ui, Icon::Refresh, "Refresh", 13.0, true).clicked()
        {
            env.actions
                .push(PlayAction::OpenBrowse(page.browse_id.clone()));
        }
    });
    ui.separator();

    match browse_state {
        AsyncState::Idle => {
            render_continue_card(ui, play_queue, &mut env);
            ui.label("Browse YouTube Music Home recommendations, Artist profiles, or Album pages.");
            if ui.button("Load Home Recommendations").clicked() {
                env.actions
                    .push(PlayAction::OpenBrowse(BROWSE_HOME.to_string()));
            }
            return actions;
        }
        AsyncState::Loading(msg) => {
            render_continue_card(ui, play_queue, &mut env);
            render_skeleton_shelves(ui, msg);
            return actions;
        }
        AsyncState::Failed(err) => {
            async_banner(ui, None, Some(err.as_str()));
            if ui.button("Retry").clicked() && !browse_target.trim().is_empty() {
                env.actions
                    .push(PlayAction::OpenBrowse(browse_target.trim().to_string()));
            }
            return actions;
        }
        AsyncState::Ready(_) => {}
    }

    let Some(page) = browse_state.data() else {
        return actions;
    };

    if browse_from_cache {
        ui.small(tr(lang, Key::HomeCachedNotice));
    }

    egui::ScrollArea::vertical().show(ui, |ui| {
        if page.kind == BrowsePageKind::Home {
            render_continue_card(ui, play_queue, &mut env);
        }
        render_header(ui, page, &mut env);
        ui.add_space(8.0);

        for (s_idx, section) in page.sections.iter().enumerate() {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.heading(&section.title);
                    if let Some(bid) = &section.browse_id
                        && ui.button("See all ->").clicked()
                    {
                        env.actions.push(PlayAction::OpenBrowseWithParams {
                            browse_id: bid.clone(),
                            params: section.browse_params.clone(),
                        });
                    }
                });
                if !section.cards.is_empty() {
                    egui::ScrollArea::horizontal()
                        .id_salt(("browse_shelf_h", &page.browse_id, s_idx))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                for card in &section.cards {
                                    render_shelf_card(ui, card, &mut env);
                                }
                            });
                        });
                }
                if !section.tracks.is_empty() {
                    for (tidx, track) in section.tracks.iter().enumerate() {
                        render_track_row(ui, track, &section.tracks, tidx, &mut env);
                    }
                }
            });
            ui.add_space(6.0);
        }
        if let Some(token) = &page.continuation {
            ui.add_space(6.0);
            if ui.button("Load More").clicked() {
                let _ = cmd_tx.send(WorkerCmd::LoadBrowseContinuation {
                    browse_id: page.browse_id.clone(),
                    token: token.clone(),
                });
            }
        }
    });

    actions
}

fn render_header(ui: &mut egui::Ui, page: &BrowsePage, env: &mut ShelfUiEnv<'_>) {
    ui.horizontal(|ui| {
        if let Some(url) = &page.thumbnail_url {
            paint_art_box(ui, url, egui::vec2(84.0, 84.0), env.artwork);
        }
        ui.vertical(|ui| {
            ui.heading(&page.title);
            if !page.artists.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for (idx, artist) in page.artists.iter().enumerate() {
                        if idx > 0 {
                            ui.small(",");
                        }
                        if let Some(bid) = &artist.browse_id {
                            if ui.link(&artist.name).clicked() {
                                env.actions.push(PlayAction::OpenBrowse(bid.clone()));
                            }
                        } else {
                            ui.small(&artist.name);
                        }
                    }
                });
            }
            let kind_str = match page.kind {
                BrowsePageKind::Home => "Home",
                BrowsePageKind::Artist => "Artist",
                BrowsePageKind::Album => "Album",
                BrowsePageKind::Playlist => "Playlist",
                BrowsePageKind::Unknown => "Browse",
            };
            ui.small(format!("{kind_str} • {}", page.browse_id));
            if let Some(sub) = &page.subtitle {
                ui.label(sub);
            }
            if !page.tracks.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    if icon_text_button(ui, Icon::Play, "Play All", 13.0, true).clicked() {
                        env.actions.push(PlayAction::PlayFromList {
                            tracks: page.tracks.clone(),
                            start_idx: 0,
                        });
                    }
                    if ui.button("+ Queue All").clicked() {
                        for t in &page.tracks {
                            env.actions.push(PlayAction::AppendQueue(t.clone()));
                        }
                    }
                    let has_editable = env
                        .playlists
                        .iter()
                        .any(|p| p.access() == PlaylistAccess::EditableOwned);
                    if has_editable {
                        ui.menu_button("+ Stage All", |ui| {
                            for dest in env
                                .playlists
                                .iter()
                                .filter(|p| p.access() == PlaylistAccess::EditableOwned)
                            {
                                if ui.button(&dest.title).clicked() {
                                    let count = page.tracks.len();
                                    let eid = env.sync_queue.push(StagedOp::AddTracks {
                                        playlist_id: dest.id.clone(),
                                        playlist_title: dest.title.clone(),
                                        tracks: page.tracks.clone(),
                                        allow_duplicates: false,
                                    });
                                    *env.toast = Some(ToastState::staged(
                                        format!("Staged {count} tracks -> '{}'", dest.title),
                                        Some(eid),
                                    ));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                });
            }
        });
    });
}

fn render_track_row(
    ui: &mut egui::Ui,
    track: &Track,
    list: &[Track],
    idx: usize,
    env: &mut ShelfUiEnv<'_>,
) {
    ui.horizontal(|ui| {
        if icon_button(ui, Icon::Play, 12.0, true, "Play track").clicked() {
            env.actions.push(PlayAction::PlayFromList {
                tracks: list.to_vec(),
                start_idx: idx,
            });
        }
        if let Some(url) = track.effective_thumbnail_url() {
            paint_art_box(ui, &url, egui::vec2(22.0, 22.0), env.artwork);
        }
        let title_resp = ui
            .add(
                egui::Label::new(egui::RichText::new(&track.title).strong())
                    .sense(egui::Sense::click()),
            )
            .on_hover_text("Double-click to play • Right-click for menu");
        if title_resp.double_clicked() {
            env.actions.push(PlayAction::PlayFromList {
                tracks: list.to_vec(),
                start_idx: idx,
            });
        }
        attach_context_menu(
            &title_resp,
            ContextTarget::Track {
                track: track.clone(),
                queue_list: list.to_vec(),
                row_idx: idx,
            },
            ContextMenuEnv {
                lang: env.lang,
                playlists: env.playlists,
                sync_queue: env.sync_queue,
                toast: env.toast,
                actions: env.actions,
            },
            None,
        );
        if let Some(artist) = track.artists.first() {
            ui.small("—");
            if let Some(bid) = &artist.browse_id {
                if ui.link(&artist.name).clicked() {
                    env.actions.push(PlayAction::OpenBrowse(bid.clone()));
                }
            } else {
                ui.small(&artist.name);
            }
        }
        if let Some(album) = &track.album {
            ui.small("•");
            if let Some(bid) = &album.browse_id {
                if ui.link(&album.name).clicked() {
                    env.actions.push(PlayAction::OpenBrowse(bid.clone()));
                }
            } else {
                ui.small(&album.name);
            }
        }
        if let Some(dur) = track.duration_sec {
            ui.small(format_duration(u64::from(dur)));
        }
        if icon_text_button(ui, Icon::Next, "Next", 12.0, true).clicked() {
            env.actions.push(PlayAction::PlayNext(track.clone()));
        }
        if ui
            .small_button("+Q")
            .on_hover_text("Add to queue")
            .clicked()
        {
            env.actions.push(PlayAction::AppendQueue(track.clone()));
        }
        if icon_button(ui, Icon::Radio, 12.0, true, "Start Radio").clicked() {
            env.actions.push(PlayAction::StartRadio(track.clone()));
        }
        let has_editable = env
            .playlists
            .iter()
            .any(|p| p.access() == PlaylistAccess::EditableOwned);
        if has_editable {
            ui.menu_button("+ Stage", |ui| {
                for dest in env
                    .playlists
                    .iter()
                    .filter(|p| p.access() == PlaylistAccess::EditableOwned)
                {
                    if ui.button(&dest.title).clicked() {
                        let eid = env.sync_queue.push(StagedOp::AddTracks {
                            playlist_id: dest.id.clone(),
                            playlist_title: dest.title.clone(),
                            tracks: vec![track.clone()],
                            allow_duplicates: false,
                        });
                        *env.toast = Some(ToastState::staged(
                            format!("Staged '{}' -> '{}'", track.title, dest.title),
                            Some(eid),
                        ));
                        ui.close_menu();
                    }
                }
            });
        }
    });
}
