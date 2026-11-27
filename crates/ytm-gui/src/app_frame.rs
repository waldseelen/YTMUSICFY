use chrono::Timelike;
use eframe::egui;

use crate::app::YtmGuiApp;
use crate::browser_launch::{build_watch_url, open_url_in_browser};
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_text_button, paint_icon};
use crate::navigation::apply_global_input;
use crate::playback_ops::{dispatch_play_actions, toggle_mute_state};
use crate::state::{AsyncState, WorkerCmd};
use crate::theme::Palette;

pub(crate) fn sync_theme_and_input(
    ctx: &egui::Context,
    app: &mut YtmGuiApp,
    current_art_url: Option<&str>,
) -> bool {
    app.theme_state.dynamic_target = current_art_url.and_then(|url| app.artwork.accent_for(url));
    let dt = ctx.input(|i| i.unstable_dt);
    if app.theme_state.sync_and_apply(
        ctx,
        app.gui_config.theme_mode,
        app.gui_config.accent_palette,
        app.gui_config.dynamic_accent,
        app.gui_config.reduced_motion,
        dt,
        crate::theme::AccessibilityConfig {
            high_contrast: app.gui_config.high_contrast,
            font_scale: app.gui_config.font_scale,
        },
    ) {
        ctx.request_repaint();
    }
    let base_ppp = *app
        .base_pixels_per_point
        .get_or_insert_with(|| ctx.pixels_per_point());
    let input_out = apply_global_input(
        ctx,
        base_ppp,
        &mut app.ui_scale,
        &mut app.active_tab,
        &mut app.selected_playlist_id,
        &mut app.focus_search_once,
        &mut app.sync_queue,
        &mut app.toast,
        &mut app.pending_modal,
        &mut app.nav_history,
    );
    if input_out.toggle_palette {
        app.palette_open = !app.palette_open;
    }
    if input_out.toggle_sync_drawer {
        app.sync_drawer_open = !app.sync_drawer_open;
    }
    if input_out.toggle_play_pause {
        app.player.toggle_pause();
    }
    if input_out.toggle_mute {
        toggle_mute_state(app);
    }
    if input_out.toggle_mini_player {
        app.mini_player.open = !app.mini_player.open;
    }
    if input_out.open_ytm {
        app.open_current_track_in_youtube(false);
    }
    if input_out.open_ytm_at_time {
        app.open_current_track_in_youtube(true);
    }
    if let Some(url) = app.pending_copy_url.take() {
        ctx.copy_text(url);
    }
    input_out.navigated_via_history
}

impl YtmGuiApp {
    pub(crate) fn drain_toast_to_bus(&mut self) {
        if let Some(t) = self.toast.take() {
            let now = self.now_ms();
            if t.offer_sync_link || t.undo_entry_id.is_some() {
                self.status_bus.push_staged(t.message, t.undo_entry_id, now);
            } else {
                self.status_bus.push_info(t.ok, t.message, now);
            }
        }
    }

    pub(crate) fn trigger_session_refresh(&mut self) {
        let vid = self.play_queue.current_track().map(|t| t.video_id.as_str());
        let url = build_watch_url(vid);
        self.session_refresh.begin();
        let browser = self.gui_config.preferred_external_browser.as_deref();
        match open_url_in_browser(&url, browser) {
            Ok(()) => {
                let now = self.now_ms();
                self.session_refresh.mark_waiting(now);
                self.status_bus
                    .push_hint(tr(self.lang(), Key::AuthRefreshingWait), now);
            }
            Err(err) => {
                let msg = format!("Open browser failed: {err}");
                let now = self.now_ms();
                self.session_refresh.finish_err(&msg);
                self.status_bus.push_info(false, msg, now);
            }
        }
    }

    pub(crate) fn open_current_track_in_youtube(&mut self, with_timestamp: bool) {
        let Some(track) = self.play_queue.current_track() else {
            return;
        };
        let at_sec = with_timestamp.then(|| self.player.snapshot().position_sec.max(0.0) as u64);
        let url = crate::youtube_links::ytm_track_url(&track.video_id, at_sec);
        let browser = self.gui_config.preferred_external_browser.as_deref();
        if let Err(err) = open_url_in_browser(&url, browser) {
            let now = self.now_ms();
            self.status_bus
                .push_info(false, format!("Open browser failed: {err}"), now);
        }
    }
}

pub(crate) fn show_session_offline_banner(
    ctx: &egui::Context,
    app: &mut YtmGuiApp,
    palette: &Palette,
    lang: ResolvedLang,
) {
    let session_expired = matches!(app.session, AsyncState::Failed(_));
    if !session_expired && !app.offline_mode {
        return;
    }
    let mut do_refresh = false;
    egui::TopBottomPanel::top("session_offline_banner").show(ctx, |ui| {
        ui.horizontal(|ui| {
            let banner_col = palette.status_warn;
            paint_icon(ui, Icon::Warn, 14.0, banner_col);
            let msg = if session_expired {
                tr(lang, Key::AuthBannerExpired)
            } else {
                tr(lang, Key::OfflineUsingCache)
            };
            ui.colored_label(banner_col, msg);
            if icon_text_button(
                ui,
                Icon::Refresh,
                tr(lang, Key::AuthRefreshSession),
                13.0,
                !app.session_refresh.is_busy(),
            )
            .clicked()
            {
                do_refresh = true;
            }
        });
    });
    if do_refresh {
        app.trigger_session_refresh();
    }
}

/// Renders dropdowns, modals, command palette, session-refresh timer, and schedules repaints.
pub(crate) fn finish_frame(
    app: &mut YtmGuiApp,
    ctx: &eframe::egui::Context,
    navigated_via_history: bool,
) {
    let palette = app.theme_state.palette;
    let lang = app.lang();
    let now_ms = app.now_ms();

    let prev_recent_len = app.gui_config.recent_searches.len();
    let prev_recent = app.gui_config.recent_searches.clone();
    let dd_actions = crate::views::search_dropdown::show_search_dropdown(
        ctx,
        crate::views::search_dropdown::SearchDropdownCtx {
            anchor_rect: app.search_bar_rect,
            ctrl: &mut app.search_controller,
            palette: &palette,
            lang,
            catalog_query: &mut app.catalog_query,
            catalog_filter: app.catalog_filter,
            active_tab: &mut app.active_tab,
            recent_persisted: &mut app.gui_config.recent_searches,
            cmd_tx: &app.cmd_tx,
        },
    );
    if app.gui_config.recent_searches.len() != prev_recent_len
        || app.gui_config.recent_searches != prev_recent
    {
        let _ = app
            .cmd_tx
            .send(WorkerCmd::UpdateGuiConfig(app.gui_config.clone()));
    }
    if !dd_actions.is_empty() {
        dispatch_play_actions(dd_actions, app);
    }

    if app.session_refresh.should_import(now_ms) {
        app.session_refresh.mark_importing();
        let _ = app
            .cmd_tx
            .send(WorkerCmd::ImportBrowserAuth(app.auth_browser));
    }

    let playlists_slice = app.library.data().map(Vec::as_slice).unwrap_or(&[]);
    let pos_sec = app.player.snapshot().position_sec.max(0.0) as u64;
    let current_track_info = app.play_queue.current_track().map(|t| (t, pos_sec));
    crate::command_palette::show_command_palette(
        ctx,
        &mut app.palette_open,
        &mut app.palette_query,
        playlists_slice,
        &mut app.active_tab,
        &mut app.selected_playlist_id,
        &mut app.sync_drawer_open,
        &mut app.loading_playlists,
        &app.playlist_cache,
        app.export_fmt,
        &app.export_dir,
        current_track_info,
        app.gui_config.preferred_external_browser.as_deref(),
        &app.cmd_tx,
    );
    app.focus_search_once = false;
    if !navigated_via_history {
        app.nav_history
            .record(app.active_tab, app.selected_playlist_id.clone());
    }

    crate::overlays::show_drop_prompt(
        ctx,
        &mut app.drop_prompt,
        &app.playlist_cache,
        &app.library,
        &mut app.sync_queue,
        &mut app.toast,
    );
    crate::overlays::show_confirmation_modal(ctx, &mut app.pending_modal, &app.cmd_tx);
    crate::playlist_meta_modal::show_playlist_meta_modal(
        ctx,
        &mut app.meta_modal,
        &mut app.sync_queue,
        &mut app.toast,
    );
    let harvest_playlists = app.library.data().map(Vec::as_slice).unwrap_or(&[]);
    crate::views::artist_harvest::show_artist_harvest_modal(
        ctx,
        &mut app.artist_harvest_modal,
        harvest_playlists,
        &app.playlist_cache,
        &mut app.sync_queue,
        &mut app.toast,
    );
    crate::lightbox::show_lightbox(ctx, &mut app.lightbox, &mut app.artwork, &palette, lang);

    app.drain_toast_to_bus();
    crate::window_lifecycle::process_tray_and_close(ctx, app);
    crate::views::mini_player_view::show_mini_player_viewport(ctx, app);
    #[cfg(feature = "screenshot-harness")]
    if (!app.mini_player.open || ctx.embed_viewports())
        && crate::screenshot_seed::maybe_capture_screenshot(ctx, ctx)
    {
        app.explicit_quit = true;
    }

    if app.sync_queue.revision() != app.last_persisted_rev {
        app.mark_staged_persisted();
        let _ = app
            .cmd_tx
            .send(WorkerCmd::SaveStagedOps(app.sync_queue.entries().to_vec()));
    }

    let window_active =
        ctx.input(|i| i.viewport().minimized != Some(true) && i.viewport().focused != Some(false));
    app.player.set_window_active(window_active);

    let is_playing = matches!(
        app.player.snapshot().state,
        crate::player::PlayerState::Playing { .. }
    );
    if app.status_bus.current(now_ms).is_none()
        && let Some(phrase) = app
            .greeting_scheduler
            .maybe_pick(crate::greetings::GreetingContext {
                enabled: app.gui_config.greetings_enabled,
                is_playing,
                window_focused: window_active,
                now_ms,
                local_hour: chrono::Local::now().hour(),
                lang,
                rng_index: (now_ms / 1000) as usize,
            })
    {
        app.status_bus.push_greeting(phrase, now_ms);
    }

    let actively_working = app.status_bus.current(now_ms).is_some()
        || app.batch_progress.is_some()
        || app.session.is_loading()
        || app.library.is_loading()
        || app.session_refresh.is_busy()
        || !app.loading_playlists.is_empty()
        || app.mini_player.open
        || (!app.main_window_visible && app.player.snapshot().state.is_active())
        || app.player.snapshot().state.is_active();
    if actively_working {
        let interval_ms = if window_active { 500 } else { 1500 };
        ctx.request_repaint_after(std::time::Duration::from_millis(interval_ms));
    }
}
