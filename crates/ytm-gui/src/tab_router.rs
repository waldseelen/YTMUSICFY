use eframe::egui;

use crate::app::YtmGuiApp;
use crate::state::GuiTab;
use crate::views;

pub fn show_central_panel(ui: &mut egui::Ui, app: &mut YtmGuiApp) {
    let sync_len = app.sync_queue.len();
    ui.horizontal_wrapped(|ui| {
        for tab in GuiTab::ALL {
            let label = if tab == GuiTab::Sync && sync_len > 0 {
                format!("Sync ({sync_len})")
            } else {
                tab.label().to_string()
            };
            ui.selectable_value(&mut app.active_tab, tab, label);
        }
    });
    ui.separator();

    let sel_id = app.selected_playlist_id.as_deref();
    let draft_active =
        sel_id.and_then(|id| app.sync_queue.working_playlist(id, &app.playlist_cache));
    let playlists = app.library.data().map(Vec::as_slice).unwrap_or(&[]);

    match app.active_tab {
        GuiTab::Inspector => {
            let is_loading = sel_id.is_some_and(|id| app.loading_playlists.contains(id));
            let load_err = sel_id
                .and_then(|id| app.playlist_errors.get(id))
                .map(String::as_str);
            views::inspect::show(
                ui,
                draft_active.as_ref(),
                &mut app.selected_playlist_id,
                is_loading,
                load_err,
                playlists,
                &app.library,
                &mut app.loading_playlists,
                &mut app.playlist_cache,
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
                &mut app.toast,
                app.focus_search_once,
                &app.cmd_tx,
            );
        }
        GuiTab::Dedupe => views::dedupe::show(
            ui,
            sel_id,
            &mut app.dedupe_form,
            &mut app.dedupe_plan,
            &mut app.sync_queue,
            &mut app.toast,
            &app.cmd_tx,
        ),
        GuiTab::Sort => views::sort::show(
            ui,
            sel_id,
            &mut app.sort_form,
            &mut app.sort_plan,
            &app.playlist_cache,
            &mut app.sync_queue,
            &mut app.toast,
            &app.cmd_tx,
        ),
        GuiTab::SetOps => views::set_ops::show(
            ui,
            playlists,
            &mut app.set_form,
            &mut app.set_plan,
            &mut app.sync_queue,
            &mut app.toast,
            &app.cmd_tx,
        ),
        GuiTab::Health => views::health::show(
            ui,
            draft_active.as_ref(),
            sel_id,
            &mut app.health_form,
            &mut app.health_report,
            &mut app.sync_queue,
            &mut app.toast,
            &app.cmd_tx,
        ),
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
            &mut app.pending_modal,
            &app.cmd_tx,
        ),
        GuiTab::Sync => views::sync::show(
            ui,
            &mut app.sync_queue,
            &mut app.pending_modal,
            &mut app.toast,
        ),
        GuiTab::Auth => views::auth::show(
            ui,
            &app.session,
            &mut app.auth_raw_input,
            &mut app.auth_browser,
            &app.cmd_tx,
        ),
    }
}
