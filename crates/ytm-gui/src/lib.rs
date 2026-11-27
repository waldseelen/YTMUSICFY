pub mod app;
pub mod app_events;
pub mod app_frame;
pub mod artwork;
pub mod auth_store;
pub mod brand;
pub mod browser_launch;
pub mod command_palette;
pub mod contracts;
pub mod draft_cache;
pub mod greetings;
pub mod i18n;
pub mod icons;
pub mod interaction;
pub mod lightbox;
pub mod media_controls;
pub mod mini_player;
pub mod navigation;
pub mod overlays;
pub mod play_queue;
pub mod playback_ops;
pub mod player;
pub mod player_ipc;
pub mod playlist_meta_modal;
pub mod right_drawer;
#[cfg(feature = "screenshot-harness")]
pub mod screenshot_seed;
pub mod search_controller;
pub mod shortcuts;
pub mod sidebar;
pub mod state;
pub mod status_bus;
pub mod status_ribbon;
pub mod studio_menu;
pub mod sync_queue;
pub mod tab_bar;
pub mod tab_router;
pub mod theme;
pub mod theme_accent;
pub mod theme_detect;
pub mod top_bar;
pub mod tray;
#[cfg(target_os = "linux")]
mod tray_linux;
#[cfg(windows)]
mod tray_windows;
pub mod views;
pub mod window_lifecycle;
pub mod worker;
pub mod worker_auth;
pub mod worker_exec;
pub mod worker_media;
pub mod worker_msgs;
pub mod worker_plans;
pub mod youtube_links;

use std::ffi::OsStr;
use std::sync::Arc;

use anyhow::Result;
use eframe::egui;
use tracing_subscriber::EnvFilter;

use crate::app::YtmGuiApp;

/// Wayland Mesa EGL `eglSwapBuffers` with `vsync: true` blocks the main thread on `wl_surface.frame`
/// whenever the window is hidden/occluded on another workspace, triggering compositor ANR dialogs.
fn should_enable_vsync(wayland_display: Option<&OsStr>) -> bool {
    !(cfg!(target_os = "linux") && wayland_display.is_some_and(|v| !v.is_empty()))
}

/// Launch the Dalga desktop GUI application.
pub fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id("ytm-architect")
        .with_inner_size([1280.0, 820.0])
        .with_min_inner_size([960.0, 640.0])
        .with_title(brand::WINDOW_TITLE);
    if let Some(icon) = brand::load_window_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        vsync: should_enable_vsync(std::env::var_os("WAYLAND_DISPLAY").as_deref()),
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        brand::WINDOW_TITLE,
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::apply_theme(&cc.egui_ctx);
            #[cfg(feature = "screenshot-harness")]
            let app = {
                let mut a = YtmGuiApp::new(cc);
                screenshot_seed::apply_preview_scene(&mut a);
                a
            };
            #[cfg(not(feature = "screenshot-harness"))]
            let app = YtmGuiApp::new(cc);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe launch failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vsync_disabled_on_linux_wayland_only() {
        assert!(should_enable_vsync(None));
        assert!(should_enable_vsync(Some(OsStr::new(""))));
        assert_eq!(
            should_enable_vsync(Some(OsStr::new("wayland-1"))),
            !cfg!(target_os = "linux")
        );
    }
}
