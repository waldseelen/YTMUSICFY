pub mod app;
pub mod app_events;
pub mod artwork;
pub mod auth_store;
pub mod brand;
pub mod command_palette;
pub mod draft_cache;
pub mod icons;
pub mod interaction;
pub mod media_controls;
pub mod navigation;
pub mod overlays;
pub mod play_queue;
pub mod playback_ops;
pub mod player;
pub mod player_ipc;
pub mod playlist_meta_modal;
pub mod sidebar;
pub mod state;
pub mod sync_queue;
pub mod tab_bar;
pub mod tab_router;
pub mod theme;
pub mod views;
pub mod worker;
pub mod worker_auth;
pub mod worker_exec;
pub mod worker_media;
pub mod worker_msgs;
pub mod worker_plans;

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
            theme::apply_theme(&cc.egui_ctx);
            Ok(Box::new(YtmGuiApp::new(cc)))
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
