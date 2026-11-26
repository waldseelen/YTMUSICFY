use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::{Playlist, PlaylistPrivacy, PlaylistSummary};

use crate::draft_cache::DraftCache;
use crate::interaction::{auto_scroll_on_drag, search_box_with_clear};
use crate::navigation::ToastState;
use crate::overlays::handle_cross_playlist_drop;
use crate::playlist_meta_modal::PlaylistMetaModal;
use crate::state::{
    AsyncState, ConfirmedMutation, DragDropMode, DragTrackPayload, DropPrompt, GuiTab,
    PendingMutation, SetOpsForm, SidebarSearchMode, SplitPaneState, WorkerCmd,
};
use crate::sync_queue::{StagedOp, SyncQueue};
use crate::views::repertoire_tree::collect_repertoire_matches;

#[allow(clippy::too_many_arguments)]
pub fn show(
    ui: &mut egui::Ui,
    library: &AsyncState<Vec<PlaylistSummary>>,
    sidebar_filter: &mut String,
    search_mode: &mut SidebarSearchMode,
    drag_mode: DragDropMode,
    new_playlist_title: &mut String,
    selected_playlist_id: &mut Option<String>,
    loading_playlists: &mut HashSet<String>,
    playlist_cache: &mut HashMap<String, Playlist>,
    draft_cache: &mut DraftCache,
    sync_queue: &mut SyncQueue,
    split_state: &mut SplitPaneState,
    set_form: &mut SetOpsForm,
    active_tab: &mut GuiTab,
    drop_prompt: &mut Option<DropPrompt>,
    meta_modal: &mut Option<PlaylistMetaModal>,
    pending_modal: &mut Option<PendingMutation>,
    toast: &mut Option<ToastState>,
    focus_filter: bool,
    focused_idx: &mut Option<usize>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.horizontal(|ui| {
        ui.strong("Library");
        ui.selectable_value(search_mode, SidebarSearchMode::Playlists, "Lists")
            .on_hover_text("Filter sidebar playlist titles");
        if ui
            .selectable_value(search_mode, SidebarSearchMode::AllTracks, "All Tracks")
            .on_hover_text("Search artist/title across ALL playlists and open File-Tree view")
            .clicked()
        {
            *active_tab = GuiTab::Inspector;
        }
    });
    let hint = match *search_mode {
        SidebarSearchMode::Playlists => "Filter playlists...",
        SidebarSearchMode::AllTracks => "Search artist/song across all lists...",
    };
    let prev_filter = sidebar_filter.clone();
    let box_w = (ui.available_width() - 42.0).clamp(140.0, 220.0);
    search_box_with_clear(
        ui,
        "sidebar_filter_box",
        sidebar_filter,
        hint,
        box_w,
        focus_filter,
    );
    if *search_mode == SidebarSearchMode::AllTracks
        && *sidebar_filter != prev_filter
        && !sidebar_filter.trim().is_empty()
    {
        *active_tab = GuiTab::Inspector;
    }
    if *search_mode == SidebarSearchMode::AllTracks {
        let total_pl = library.data().map_or(0, Vec::len);
        let cached_pl = library.data().map_or(0, |list| {
            list.iter()
                .filter(|p| playlist_cache.contains_key(&p.id))
                .count()
        });
        ui.horizontal(|ui| {
            ui.small(format!("Indexed: {cached_pl}/{total_pl}"));
            if ui
                .small_button("Index All")
                .on_hover_text(
                    "Download uncached playlists locally (350ms paced) for offline search",
                )
                .clicked()
            {
                let _ = cmd_tx.send(WorkerCmd::IndexAllPlaylists { missing_only: true });
            }
        });
    }
    ui.horizontal(|ui| {
        let new_resp = ui.add(
            egui::TextEdit::singleline(new_playlist_title)
                .hint_text("New playlist title...")
                .desired_width(145.0),
        );
        let enter_create = new_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let title = new_playlist_title.trim().to_string();
        if !title.is_empty()
            && (enter_create || ui.add_enabled(true, egui::Button::new("+ Stage")).clicked())
        {
            let eid = sync_queue.push(StagedOp::CreatePlaylist {
                title: title.clone(),
                description: "Created by ytm-gui".to_string(),
                privacy: PlaylistPrivacy::Private,
                tracks: Vec::new(),
            });
            *toast = Some(ToastState::staged(
                format!("Staged new playlist '{title}' in Sync."),
                Some(eid),
            ));
            new_playlist_title.clear();
        } else if title.is_empty() {
            ui.add_enabled(false, egui::Button::new("+ Stage"));
        }
    });
    ui.separator();

    if let AsyncState::Loading(m) = library {
        ui.spinner();
        ui.label(m);
        return;
    }
    if let AsyncState::Failed(err) = library {
        ui.colored_label(egui::Color32::from_rgb(235, 95, 95), err);
        if ui.button("Retry Library").clicked() {
            let _ = cmd_tx.send(WorkerCmd::RefreshLibrary);
        }
        return;
    }
    let Some(playlists) = library.data() else {
        return;
    };

    let dirty_ids = sync_queue.affected_playlist_ids();
    let needle = sidebar_filter.trim().to_ascii_lowercase();
    let track_hit_counts: HashMap<String, usize> =
        if *search_mode == SidebarSearchMode::AllTracks && !needle.is_empty() {
            collect_repertoire_matches(&needle, playlists, playlist_cache, sync_queue)
                .into_iter()
                .map(|m| (m.playlist.id, m.tracks.len()))
                .collect()
        } else {
            HashMap::new()
        };
    let filtered: Vec<&PlaylistSummary> = playlists
        .iter()
        .filter(|pl| {
            if needle.is_empty() {
                true
            } else if *search_mode == SidebarSearchMode::AllTracks {
                track_hit_counts.contains_key(&pl.id)
                    || pl.title.to_ascii_lowercase().contains(&needle)
            } else {
                pl.title.to_ascii_lowercase().contains(&needle)
            }
        })
        .collect();

    let sidebar_active = ui.rect_contains_pointer(ui.max_rect())
        || *active_tab != GuiTab::Inspector
        || selected_playlist_id.is_none();
    if sidebar_active && !ui.ctx().wants_keyboard_input() && !filtered.is_empty() {
        let (up, down, enter) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::Enter),
            )
        });
        if down {
            *focused_idx = Some(focused_idx.map_or(0, |idx| (idx + 1).min(filtered.len() - 1)));
        } else if up {
            *focused_idx = Some(focused_idx.map_or(0, |idx| idx.saturating_sub(1)));
        }
        if enter
            && let Some(idx) = *focused_idx
            && let Some(pl) = filtered.get(idx)
        {
            select_playlist(
                pl.id.clone(),
                selected_playlist_id,
                playlist_cache,
                loading_playlists,
                cmd_tx,
            );
        }
    }

    let pointer_pos = ui.input(|i| i.pointer.hover_pos()).unwrap_or_default();
    let scroll_out = egui::ScrollArea::vertical().show(ui, |ui| {
        for (vis_idx, pl) in filtered.iter().enumerate() {
            let selected = selected_playlist_id.as_deref() == Some(pl.id.as_str());
            let kb_focused = *focused_idx == Some(vis_idx);
            let draft_count = draft_cache
                .track_count(&pl.id, sync_queue, playlist_cache)
                .or(pl.track_count);
            let count = draft_count.map_or_else(|| "?".to_string(), |c| c.to_string());
            let dirty_mark = if dirty_ids.contains(&pl.id) { "*" } else { "" };
            let loading_mark = if loading_playlists.contains(&pl.id) {
                " ..."
            } else {
                ""
            };
            let hit_mark = track_hit_counts
                .get(&pl.id)
                .map_or_else(String::new, |h| format!(" [{h}★]"));
            let label = format!("{dirty_mark}{} ({count}){hit_mark}{loading_mark}", pl.title);

            let resp = ui.add(
                egui::Button::new(label)
                    .selected(selected || kb_focused)
                    .frame(selected || kb_focused)
                    .truncate(),
            );
            if kb_focused {
                resp.scroll_to_me(Some(egui::Align::Center));
            }
            if resp.dnd_hover_payload::<DragTrackPayload>().is_some() {
                ui.painter().rect_stroke(
                    resp.rect.expand(1.0),
                    2.0,
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgb(90, 175, 240)),
                    egui::StrokeKind::Inside,
                );
            }
            if let Some(payload) = resp.dnd_release_payload::<DragTrackPayload>()
                && payload.source_playlist_id != pl.id
            {
                let src_title = playlists
                    .iter()
                    .find(|p| p.id == payload.source_playlist_id)
                    .map_or_else(|| payload.source_playlist_id.clone(), |p| p.title.clone());
                let prompt = DropPrompt {
                    pos: pointer_pos,
                    source_playlist_id: payload.source_playlist_id.clone(),
                    source_playlist_title: src_title,
                    target_playlist_id: pl.id.clone(),
                    target_playlist_title: pl.title.clone(),
                    tracks: payload.tracks.clone(),
                };
                handle_cross_playlist_drop(
                    ui.ctx(),
                    prompt,
                    drag_mode,
                    drop_prompt,
                    playlist_cache,
                    library,
                    sync_queue,
                    toast,
                );
            }

            if resp.clicked() {
                *focused_idx = Some(vis_idx);
                select_playlist(
                    pl.id.clone(),
                    selected_playlist_id,
                    playlist_cache,
                    loading_playlists,
                    cmd_tx,
                );
            }
            if resp.double_clicked() {
                *active_tab = GuiTab::Inspector;
            }

            resp.context_menu(|ui| {
                if ui.button("Open in Main Pane").clicked() {
                    *active_tab = GuiTab::Inspector;
                    select_playlist(
                        pl.id.clone(),
                        selected_playlist_id,
                        playlist_cache,
                        loading_playlists,
                        cmd_tx,
                    );
                    ui.close_menu();
                }
                if ui.button("Open in Split Pane B (Diff)").clicked() {
                    split_state.enabled = true;
                    split_state.playlist_b_id = Some(pl.id.clone());
                    *active_tab = GuiTab::Inspector;
                    if !playlist_cache.contains_key(&pl.id)
                        && loading_playlists.insert(pl.id.clone())
                    {
                        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                    }
                    ui.close_menu();
                }
                if ui.button("Set as Diff A").clicked() {
                    set_form.playlist_a = pl.id.clone();
                    ui.close_menu();
                }
                if ui.button("Set as Diff B").clicked() {
                    set_form.playlist_b = pl.id.clone();
                    ui.close_menu();
                }
                if ui.button("Force Refresh from YTM").clicked() {
                    playlist_cache.remove(&pl.id);
                    draft_cache.invalidate_playlist(&pl.id);
                    if loading_playlists.insert(pl.id.clone()) {
                        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                    }
                    ui.close_menu();
                }
                let editable_pl = pl.owned && !pl.id.eq_ignore_ascii_case("LM");
                if editable_pl {
                    ui.separator();
                    let hydrated = draft_cache.get_or_compute(&pl.id, sync_queue, playlist_cache);
                    let edit_btn = ui
                        .add_enabled(
                            hydrated.is_some(),
                            egui::Button::new("Edit Details / Privacy..."),
                        )
                        .on_disabled_hover_text(
                            "Load playlist first so description and privacy are hydrated.",
                        );
                    if edit_btn.clicked()
                        && let Some(ref cached) = hydrated
                    {
                        *meta_modal = Some(PlaylistMetaModal::new(
                            cached.id.clone(),
                            cached.title.clone(),
                            cached.description.clone().unwrap_or_default(),
                            cached.privacy,
                        ));
                        ui.close_menu();
                    }
                    if hydrated.is_none() && ui.button("Load Details to Edit...").clicked() {
                        if loading_playlists.insert(pl.id.clone()) {
                            let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(pl.id.clone()));
                        }
                        *toast = Some(ToastState::info(
                            true,
                            format!("Loading '{}' metadata before editing...", pl.title),
                        ));
                        ui.close_menu();
                    }
                    if ui.button("Delete Playlist...").clicked() {
                        *pending_modal = Some(PendingMutation {
                            title: format!("Delete Playlist '{}'", pl.title),
                            summary: format!(
                                "Permanently delete '{}' ({}) on YouTube Music? A local pre-delete SQLite snapshot will be saved first.",
                                pl.title, pl.id
                            ),
                            action: ConfirmedMutation::DeletePlaylist {
                                playlist_id: pl.id.clone(),
                                playlist_title: pl.title.clone(),
                            },
                        });
                        ui.close_menu();
                    }
                }
            });
        }
        auto_scroll_on_drag(ui, ui.clip_rect());
    });
    let _ = scroll_out;
}

fn select_playlist(
    id: String,
    selected_playlist_id: &mut Option<String>,
    playlist_cache: &HashMap<String, Playlist>,
    loading_playlists: &mut HashSet<String>,
    cmd_tx: &Sender<WorkerCmd>,
) {
    *selected_playlist_id = Some(id.clone());
    if !playlist_cache.contains_key(&id) && loading_playlists.insert(id.clone()) {
        let _ = cmd_tx.send(WorkerCmd::LoadPlaylist(id));
    }
}
