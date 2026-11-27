use eframe::egui;

use crate::app::YtmGuiApp;
use crate::contracts::CloseAction;
use crate::playback_ops::start_playing_track;
use crate::player::PlayerState;
use crate::tray::{TrayCommand, TraySnapshot};

/// Pure close-request reducer deciding between ignoring, hiding to tray, or quitting.
#[must_use]
pub const fn resolve_close_request(
    close_requested: bool,
    tray_active: bool,
    close_to_tray: bool,
    explicit_quit: bool,
) -> CloseAction {
    if !close_requested {
        return CloseAction::None;
    }
    if explicit_quit || !tray_active || !close_to_tray {
        CloseAction::Quit
    } else {
        CloseAction::HideToTray
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
    match resolve_close_request(
        close_requested,
        app.tray_ctrl.is_active(),
        app.gui_config.close_to_tray,
        app.explicit_quit,
    ) {
        CloseAction::None | CloseAction::Quit => {}
        CloseAction::HideToTray => {
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
    fn close_request_reducer_hides_to_tray_only_when_tray_active_and_not_explicit_quit() {
        assert_eq!(
            resolve_close_request(false, true, true, false),
            CloseAction::None
        );
        assert_eq!(
            resolve_close_request(true, true, true, false),
            CloseAction::HideToTray
        );
        assert_eq!(
            resolve_close_request(true, true, false, false),
            CloseAction::Quit
        );
        assert_eq!(
            resolve_close_request(true, false, true, false),
            CloseAction::Quit
        );
        assert_eq!(
            resolve_close_request(true, true, true, true),
            CloseAction::Quit
        );
    }
}
