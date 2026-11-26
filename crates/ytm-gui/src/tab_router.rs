use eframe::egui;

use crate::app::YtmGuiApp;
use crate::playback_ops::dispatch_play_actions;
use crate::state::{AsyncState, GuiTab, SidebarSearchMode, WorkerCmd};
use crate::tab_bar::show_tab_bar;
use crate::views;

pub fn show_central_panel(ui: &mut egui::Ui, app: &mut YtmGuiApp) {
    let sync_len = app.sync_queue.len();
    show_tab_bar(ui, &mut app.active_tab, sync_len, &mut app.sync_drawer_open);
    ui.separator();

    let sel_id = app.selected_playlist_id.as_deref();
    let draft_active = sel_id.and_then(|id| {
        app.draft_cache
            .get_or_compute(id, &app.sync_queue, &app.playlist_cache)
    });
    let playlists = app.library.data().map(Vec::as_slice).unwrap_or(&[]);

    let mut pending_actions = Vec::new();
    match app.active_tab {
        GuiTab::Browse => {
            if matches!(app.browse_state, AsyncState::Idle) && app.session.data().is_some() {
                app.browse_state = AsyncState::Loading(format!("Loading {}...", app.browse_target));
                let _ = app.cmd_tx.send(WorkerCmd::LoadBrowse {
                    browse_id: app.browse_target.clone(),
                });
            }
            pending_actions = views::browse::show(
                ui,
                &mut app.browse_target,
                &app.browse_state,
                playlists,
                &mut app.sync_queue,
                &mut app.artwork,
                &mut app.toast,
                &app.cmd_tx,
            );
        }
        GuiTab::Search => {
            pending_actions = views::search::show(
                ui,
                &mut app.catalog_query,
                &mut app.catalog_filter,
                &mut app.catalog_results,
                playlists,
                &mut app.sync_queue,
                &mut app.artwork,
                &mut app.toast,
                &app.cmd_tx,
            );
        }
        GuiTab::QueueLyrics => {
            pending_actions = views::queue_lyrics::show(ui, app);
        }
        GuiTab::Inspector => {
            if app.sidebar_search_mode == SidebarSearchMode::AllTracks
                && !app.sidebar_filter.trim().is_empty()
            {
                pending_actions = views::repertoire_tree::show(
                    ui,
                    &mut app.sidebar_filter,
                    &mut app.sidebar_search_mode,
                    playlists,
                    &app.library,
                    &mut app.loading_playlists,
                    &app.playlist_cache,
                    &mut app.sync_queue,
                    &mut app.repertoire_tree,
                    &mut app.drag_mode,
                    &mut app.selected_playlist_id,
                    &mut app.active_tab,
                    &mut app.drop_prompt,
                    &mut app.toast,
                    &app.cmd_tx,
                );
            } else {
                let is_loading = sel_id.is_some_and(|id| app.loading_playlists.contains(id));
                let load_err = sel_id
                    .and_then(|id| app.playlist_errors.get(id))
                    .map(String::as_str);
                let manifest = sel_id.and_then(|id| app.playlist_manifests.get(id));
                pending_actions = views::inspect::show(
                    ui,
                    draft_active.as_deref(),
                    manifest,
                    &mut app.selected_playlist_id,
                    is_loading,
                    load_err,
                    playlists,
                    &app.library,
                    &mut app.loading_playlists,
                    &mut app.playlist_cache,
                    &mut app.draft_cache,
                    &mut app.sync_queue,
                    &mut app.inspect_sort,
                    &mut app.inspect_selected,
                    &mut app.inspect_last_clicked,
                    &mut app.inspect_focused_row,
                    &mut app.inspect_jump_input,
                    &mut app.inspect_search,
                    &mut app.ytm_query,
                    &app.search_results,
                    &mut app.export_dir,
                    &mut app.export_fmt,
                    &mut app.split_state,
                    &mut app.drop_prompt,
                    &mut app.meta_modal,
                    &mut app.artwork,
                    &mut app.toast,
                    app.focus_search_once,
                    &app.cmd_tx,
                );
            }
        }
        GuiTab::Dedupe => {
            let manifest = sel_id.and_then(|id| app.playlist_manifests.get(id));
            pending_actions = views::dedupe::show(
                ui,
                draft_active.as_deref(),
                manifest,
                sel_id,
                &mut app.dedupe_form,
                &mut app.dedupe_plan,
                &mut app.sync_queue,
                &mut app.toast,
                &app.cmd_tx,
            );
        }
        GuiTab::Sort => {
            let manifest = sel_id.and_then(|id| app.playlist_manifests.get(id));
            views::sort::show(
                ui,
                draft_active.as_deref(),
                manifest,
                sel_id,
                &mut app.sort_form,
                &mut app.sort_plan,
                &app.playlist_cache,
                &mut app.sync_queue,
                &mut app.toast,
                &app.cmd_tx,
            );
        }
        GuiTab::SetOps => {
            pending_actions = views::set_ops::show(
                ui,
                playlists,
                &app.playlist_manifests,
                &mut app.set_form,
                &mut app.set_plan,
                &mut app.sync_queue,
                &mut app.toast,
                &app.cmd_tx,
            );
        }
        GuiTab::Health => {
            let manifest = sel_id.and_then(|id| app.playlist_manifests.get(id));
            pending_actions = views::health::show(
                ui,
                draft_active.as_deref(),
                manifest,
                sel_id,
                &mut app.health_form,
                &mut app.health_report,
                &mut app.sync_queue,
                &mut app.toast,
                &app.cmd_tx,
            );
        }
        GuiTab::GenreSplit => views::genres_snapshots::show_genre_split(
            ui,
            sel_id,
            &mut app.genre_form,
            &mut app.genre_plan,
            &mut app.sync_queue,
            &mut app.toast,
            &app.cmd_tx,
        ),
        GuiTab::Snapshots => views::genres_snapshots::show_snapshots(
            ui,
            &app.snapshots,
            &mut app.snapshot_detail,
            &app.playlist_cache,
            &mut app.sync_queue,
            &mut app.toast,
            &app.cmd_tx,
        ),
        GuiTab::Sync => views::sync::show(
            ui,
            &mut app.sync_queue,
            &mut app.pending_modal,
            &mut app.toast,
        ),
        GuiTab::Auth => {
            let snap = app.player.snapshot();
            let pending_quality = snap
                .state
                .is_active()
                .then_some(snap.pending_configure)
                .flatten();
            if views::auth::show(
                ui,
                &app.session,
                &mut app.auth_raw_input,
                &mut app.auth_browser,
                views::auth::AuthAudioConfig {
                    quality: &mut app.audio_quality,
                    cookie_bridge_active: app.cookie_bridge_active,
                    pending_quality,
                },
                &app.cmd_tx,
            ) {
                app.player.apply_pending_configure();
            }
        }
        GuiTab::Settings => {
            views::settings::show(ui, &mut app.gui_config, &app.cmd_tx);
            app.mutation_policy = app.gui_config.mutation_policy;
        }
    }

    if !pending_actions.is_empty() {
        dispatch_play_actions(pending_actions, app);
    }
}
