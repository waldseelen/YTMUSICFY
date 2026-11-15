mod app;
mod overlays;
mod sidebar;
mod state;
mod sync_queue;
mod tab_router;
mod views;
mod worker;
mod worker_exec;
mod worker_plans;

use anyhow::Result;
use eframe::egui;
use tracing_subscriber::EnvFilter;

use crate::app::YtmGuiApp;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("ytm-architect"),
        ..Default::default()
    };

    eframe::run_native(
        "ytm-architect",
        options,
        Box::new(|cc| Ok(Box::new(YtmGuiApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe launch failed: {e}"))?;
    Ok(())
}
