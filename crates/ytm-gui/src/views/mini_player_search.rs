use eframe::egui;
use ytm_core::Track;

use crate::app::YtmGuiApp;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button};
use crate::interaction::search_box_with_clear;
use crate::playback_ops::PlayAction;
use crate::state::WorkerCmd;

/// Render the expanded Mini Player search box and top 3 playable hits sharing `app.search_controller`.
pub(crate) fn render_mini_expanded_search(
    ui: &mut egui::Ui,
    app: &mut YtmGuiApp,
    lang: ResolvedLang,
    actions: &mut Vec<PlayAction>,
) {
    let now_ms = app.now_ms();
    let mut query_buf = app.search_controller.query.clone();
    let out = search_box_with_clear(
        ui,
        "dalga_mini_search_input",
        &mut query_buf,
        tr(lang, Key::MiniPlayerSearchHint),
        ui.available_width() - 28.0,
        false,
    );
    if query_buf != app.search_controller.query {
        app.search_controller.on_query_change(&query_buf, now_ms);
    }
    let due = app.search_controller.poll_due(now_ms);
    if let Some((req_id, query)) = due.fetch_suggestions {
        let _ = app
            .cmd_tx
            .send(WorkerCmd::FetchSearchSuggestions { req_id, query });
    }
    if let Some((req_id, query)) = due.fetch_live {
        let _ = app
            .cmd_tx
            .send(WorkerCmd::SearchInstantLive { req_id, query });
    }
    if out.submitted && !query_buf.trim().is_empty() {
        app.gui_config.recent_searches = app.search_controller.record_recent(&query_buf);
    }

    let preview = app.search_controller.grouped_preview();
    let top_songs: Vec<Track> = preview.songs.into_iter().take(3).cloned().collect();
    if top_songs.is_empty() {
        ui.weak(tr(lang, Key::SearchEnterHint));
    } else {
        for (idx, t) in top_songs.iter().enumerate() {
            ui.horizontal(|ui| {
                if icon_button(ui, Icon::Play, 12.0, true, tr(lang, Key::CtxPlayNow)).clicked() {
                    actions.push(PlayAction::PlayFromList {
                        tracks: top_songs.clone(),
                        start_idx: idx,
                    });
                }
                let label = format!("{} — {}", t.title, t.primary_artist_name().unwrap_or("-"));
                if ui.selectable_label(false, label).clicked() {
                    actions.push(PlayAction::PlayFromList {
                        tracks: top_songs.clone(),
                        start_idx: idx,
                    });
                }
            });
        }
    }
}
