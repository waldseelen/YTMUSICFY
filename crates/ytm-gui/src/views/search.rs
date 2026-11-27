use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{PlaylistAccess, PlaylistSummary, SearchFilter, SearchPage};

use crate::artwork::ArtworkCache;
use crate::i18n::ResolvedLang;
use crate::icons::{Icon, icon_button};
use crate::navigation::ToastState;
use crate::playback_ops::PlayAction;
use crate::state::{AsyncState, WorkerCmd};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::async_banner;
use crate::views::context_menu::{ContextMenuEnv, ContextTarget, attach_context_menu};
use crate::views::inspect_helpers::format_duration;

const ALL_FILTERS: [(SearchFilter, &str); 6] = [
    (SearchFilter::Top, "Top"),
    (SearchFilter::Songs, "Songs"),
    (SearchFilter::Videos, "Videos"),
    (SearchFilter::Albums, "Albums"),
    (SearchFilter::Artists, "Artists"),
    (SearchFilter::CommunityPlaylists, "Playlists"),
];

/// Full catalog search view with 6 YouTube Music filter tabs, continuation, and `+ Stage Add`.
#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    query: &mut String,
    filter: &mut SearchFilter,
    results: &mut AsyncState<SearchPage>,
    playlists: &[PlaylistSummary],
    sync_queue: &mut SyncQueue,
    artwork: &mut ArtworkCache,
    toast: &mut Option<ToastState>,
    cmd_tx: &Sender<WorkerCmd>,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();
    let mut run_search = false;

    ui.horizontal_wrapped(|ui| {
        ui.heading("Search YouTube Music");
        let resp = ui.add(
            egui::TextEdit::singleline(query)
                .desired_width(260.0)
                .hint_text("Search songs, albums, artists, playlists..."),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Search").clicked() || enter) && !query.trim().is_empty() {
            run_search = true;
        }
        if !query.trim().is_empty()
            && ui
                .button("Search My Library")
                .on_hover_text("Search across your personal playlists and saved albums")
                .clicked()
        {
            actions.push(PlayAction::SearchMyLibrary(query.trim().to_string()));
        }
    });

    ui.horizontal_wrapped(|ui| {
        for (f, label) in ALL_FILTERS {
            if ui.selectable_value(filter, f, label).clicked() && !query.trim().is_empty() {
                run_search = true;
            }
        }
    });

    if run_search {
        *results = AsyncState::Loading(format!("Searching '{}'...", query.trim()));
        let _ = cmd_tx.send(WorkerCmd::SearchCatalog {
            query: query.trim().to_string(),
            filter: *filter,
            continuation: None,
        });
    }
    ui.separator();

    match results {
        AsyncState::Idle => {
            ui.label("Enter a query and select a category tab to search YouTube Music.");
            return actions;
        }
        AsyncState::Loading(msg) => {
            async_banner(ui, Some(msg.as_str()), None);
            return actions;
        }
        AsyncState::Failed(err) => {
            async_banner(ui, None, Some(err.as_str()));
            return actions;
        }
        AsyncState::Ready(_) => {}
    }

    let Some(page) = results.data().cloned() else {
        return actions;
    };

    let editable_lists: Vec<&PlaylistSummary> = playlists
        .iter()
        .filter(|p| p.access() == PlaylistAccess::EditableOwned)
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("catalog_search_scroll")
        .show(ui, |ui| {
            if !page.tracks.is_empty() {
                ui.strong(format!("Tracks ({})", page.tracks.len()));
                for (idx, t) in page.tracks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if icon_button(ui, Icon::Play, 12.0, true, "Play now").clicked() {
                            actions.push(PlayAction::PlayFromList {
                                tracks: page.tracks.clone(),
                                start_idx: idx,
                            });
                        }
                        if let Some(url) = t.effective_thumbnail_url()
                            && let Some(tex) = artwork.texture_for(ui.ctx(), &url)
                        {
                            ui.add(
                                egui::Image::new((tex.id(), egui::vec2(22.0, 22.0)))
                                    .corner_radius(3.0),
                            );
                        }
                        let title_resp = ui.add(
                            egui::Label::new(egui::RichText::new(&t.title).strong())
                                .sense(egui::Sense::click()),
                        );
                        attach_context_menu(
                            &title_resp,
                            ContextTarget::Track {
                                track: t.clone(),
                                queue_list: page.tracks.clone(),
                                row_idx: idx,
                            },
                            ContextMenuEnv {
                                lang,
                                playlists,
                                sync_queue,
                                toast,
                                actions: &mut actions,
                            },
                            None,
                        );
                        ui.weak("—");
                        ui.label(t.primary_artist_name().unwrap_or("-"));
                        if let Some(dur) = t.duration_sec {
                            ui.monospace(format_duration(u64::from(dur)));
                        }
                        if ui
                            .small_button("+Q")
                            .on_hover_text("Add to queue")
                            .clicked()
                        {
                            actions.push(PlayAction::AppendQueue(t.clone()));
                        }
                        if !editable_lists.is_empty() {
                            ui.menu_button("+ Stage", |ui| {
                                for dest in &editable_lists {
                                    if ui.button(&dest.title).clicked() {
                                        let eid = sync_queue.push(StagedOp::AddTracks {
                                            playlist_id: dest.id.clone(),
                                            playlist_title: dest.title.clone(),
                                            tracks: vec![t.clone()],
                                            allow_duplicates: false,
                                        });
                                        *toast = Some(ToastState::staged(
                                            format!("Staged '{}' -> '{}'", t.title, dest.title),
                                            Some(eid),
                                        ));
                                        ui.close_menu();
                                    }
                                }
                            });
                        }
                    });
                }
            }

            if !page.cards.is_empty() {
                ui.add_space(6.0);
                ui.strong(format!(
                    "Albums / Artists / Playlists ({})",
                    page.cards.len()
                ));
                for card in &page.cards {
                    ui.horizontal(|ui| {
                        if let Some(url) = &card.thumbnail_url
                            && let Some(tex) = artwork.texture_for(ui.ctx(), url)
                        {
                            ui.add(
                                egui::Image::new((tex.id(), egui::vec2(26.0, 26.0)))
                                    .corner_radius(3.0),
                            );
                        }
                        let card_resp = ui.add(
                            egui::Label::new(egui::RichText::new(&card.title).strong())
                                .sense(egui::Sense::click()),
                        );
                        if let Some(target) = ContextTarget::from_browse_card(card) {
                            attach_context_menu(
                                &card_resp,
                                target,
                                ContextMenuEnv {
                                    lang,
                                    playlists,
                                    sync_queue,
                                    toast,
                                    actions: &mut actions,
                                },
                                None,
                            );
                        }
                        if !card.subtitle.is_empty() {
                            ui.weak(&card.subtitle);
                        }
                        if let Some(bid) = &card.browse_id
                            && ui.small_button("Open →").clicked()
                        {
                            actions.push(PlayAction::OpenBrowse(bid.clone()));
                        }
                    });
                }
            }

            if let Some(token) = &page.continuation {
                ui.add_space(8.0);
                if ui.button("Load More Results").clicked() {
                    let _ = cmd_tx.send(WorkerCmd::SearchCatalog {
                        query: query.trim().to_string(),
                        filter: *filter,
                        continuation: Some(token.clone()),
                    });
                }
            }
        });

    actions
}
