pub mod app;
pub mod app_events;
pub mod artwork;
pub mod auth_store;
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

use anyhow::Result;
use eframe::egui;
use tracing_subscriber::EnvFilter;

use crate::app::YtmGuiApp;

/// Launch the `ytm-architect` desktop GUI application.
pub fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("ytm-architect")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("ytm-architect"),
        ..Default::default()
    };

    eframe::run_native(
        "ytm-architect",
        options,
        Box::new(|cc| {
            theme::apply_theme(&cc.egui_ctx);
            Ok(Box::new(YtmGuiApp::new(cc)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe launch failed: {e}"))?;
    Ok(())
}
