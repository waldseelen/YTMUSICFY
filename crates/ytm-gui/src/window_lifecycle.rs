use eframe::egui;

use crate::app::YtmGuiApp;
use crate::contracts::{CloseAction, WindowCloseAction};
use crate::playback_ops::start_playing_track;
use crate::player::PlayerState;
use crate::tray::{TrayCommand, TraySnapshot};

/// Pure close-request reducer deciding action based on user preference and tray availability.
#[must_use]
pub fn resolve_close_request(
    close_requested: bool,
    tray_active: bool,
    close_action: WindowCloseAction,
    explicit_quit: bool,
) -> CloseAction {
    if !close_requested {
        return CloseAction::None;
    }
    if explicit_quit {
        return CloseAction::Quit;
    }
    match close_action {
        WindowCloseAction::Quit => CloseAction::Quit,
        WindowCloseAction::HideToTray => {
            if tray_active {
                CloseAction::HideToTray
            } else {
                // Tray not available, fall back to minimize
                CloseAction::Quit // Will be handled as minimize by caller
            }
        }
        WindowCloseAction::Minimize => CloseAction::None, // Caller handles minimize
        WindowCloseAction::HideWindow => CloseAction::HideToTray, // Caller handles hide without tray
    }
}

/// Apply tray configuration, sync tray menu/icon state, and drain pending tray commands.
pub(crate) fn process_tray_and_close(ctx: &egui::Context, app: &mut YtmGuiApp) {
    let egui_ctx = Some(ctx.clone());
    app.tray_ctrl
        .set_enabled(app.gui_config.tray_enabled, egui_ctx);

    let snap = app.player.snapshot();
    let is_playing = matches!(snap.state, PlayerState::Playing { .. });
    let track_label = app
        .play_queue
        .current_track()
        .map(|t| format!("{} — {}", t.title, t.primary_artist_name().unwrap_or("-")));
    app.tray_ctrl.sync_state(&TraySnapshot {
        window_visible: app.main_window_visible,
        is_playing,
        mini_player_open: app.mini_player.open,
        dark_theme: app.theme_state.palette.dark_mode,
        track_label,
        lang: app.lang(),
    });

    for cmd in app.tray_ctrl.poll_events() {
        match cmd {
            TrayCommand::ToggleWindow => {
                set_main_window_visible(ctx, app, !app.main_window_visible);
            }
            TrayCommand::PlayPause => app.player.toggle_pause(),
            TrayCommand::Next => {
                if let Some(nxt) = app.play_queue.advance().cloned() {
                    start_playing_track(&nxt, app);
                }
            }
            TrayCommand::Previous => {
                if let Some(prev) = app.play_queue.previous().cloned() {
                    start_playing_track(&prev, app);
                }
            }
            TrayCommand::ToggleMiniPlayer => {
                app.mini_player.open = !app.mini_player.open;
            }
            TrayCommand::Quit => {
                app.explicit_quit = true;
                if !app.main_window_visible {
                    app.main_window_visible = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    let close_requested = ctx.input(|i| i.viewport().close_requested());
    let tray_active = app.tray_ctrl.is_active();
    let close_action = app.gui_config.close_action;
    let action = resolve_close_request(
        close_requested,
        tray_active,
        close_action,
        app.explicit_quit,
    );

    match (action, close_action) {
        (CloseAction::None, WindowCloseAction::Minimize) => {
            // Minimize to taskbar
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        (CloseAction::None, WindowCloseAction::HideWindow) => {
            // Hide window, keep running
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            set_main_window_visible(ctx, app, false);
        }
        (CloseAction::None, WindowCloseAction::HideToTray) => {
            // Tray not available for HideToTray, fall back to hide window
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            set_main_window_visible(ctx, app, false);
        }
        (CloseAction::Quit, _) => {
            // Explicit quit from tray menu - let close happen
        }
        (_, WindowCloseAction::Quit) => {
            // User selected Quit in settings - let close happen
        }
        (CloseAction::HideToTray, _) => {
            // Hide to tray
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            set_main_window_visible(ctx, app, false);
        }
    }
}

pub(crate) fn set_main_window_visible(ctx: &egui::Context, app: &mut YtmGuiApp, visible: bool) {
    app.main_window_visible = visible;
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(visible));
    if visible {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }
    ctx.request_repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_request_reducer_respects_window_close_action() {
        use WindowCloseAction::*;
        // Quit always quits
        assert_eq!(
            resolve_close_request(true, true, Quit, false),
            CloseAction::Quit
        );
        assert_eq!(
            resolve_close_request(true, false, Quit, false),
            CloseAction::Quit
        );

        // HideToTray with active tray hides to tray
        assert_eq!(
            resolve_close_request(true, true, HideToTray, false),
            CloseAction::HideToTray
        );
        // HideToTray without tray falls back (caller handles)
        assert_eq!(
            resolve_close_request(true, false, HideToTray, false),
            CloseAction::Quit
        );

        // Minimize never closes
        assert_eq!(
            resolve_close_request(true, true, Minimize, false),
            CloseAction::None
        );
        assert_eq!(
            resolve_close_request(true, false, Minimize, false),
            CloseAction::None
        );

        // HideWindow hides without tray
        assert_eq!(
            resolve_close_request(true, true, HideWindow, false),
            CloseAction::HideToTray
        );
        assert_eq!(
            resolve_close_request(true, false, HideWindow, false),
            CloseAction::HideToTray
        );

        // Explicit quit always quits
        assert_eq!(
            resolve_close_request(true, true, HideToTray, true),
            CloseAction::Quit
        );
        assert_eq!(
            resolve_close_request(true, true, Minimize, true),
            CloseAction::Quit
        );

        // No close requested
        assert_eq!(
            resolve_close_request(false, true, HideToTray, false),
            CloseAction::None
        );
    }
}
