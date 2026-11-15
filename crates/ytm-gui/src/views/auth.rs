use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::SessionStatus;

use crate::state::{AsyncState, WorkerCmd};
use crate::views::async_banner;

pub fn show(
    ui: &mut egui::Ui,
    session: &AsyncState<SessionStatus>,
    raw_input: &mut String,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.heading("InnerTube Authentication");
    match session {
        AsyncState::Ready(s) => {
            ui.colored_label(
                egui::Color32::from_rgb(90, 200, 120),
                format!(
                    "Active Session: {} ({}) | authuser={} | page_id={}",
                    s.account_name.as_deref().unwrap_or("unknown"),
                    s.channel_handle.as_deref().unwrap_or("-"),
                    s.auth_user,
                    s.brand_page_id.as_deref().unwrap_or("-")
                ),
            );
        }
        AsyncState::Failed(err) => async_banner(ui, None, Some(err)),
        AsyncState::Loading(msg) => async_banner(ui, Some(msg), None),
        AsyncState::Idle => {}
    }

    ui.add_space(8.0);
    ui.label(
        "Paste Chrome DevTools Request Headers, HTTP headers, Cookie string, or browser.json:",
    );
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(raw_input)
                    .desired_width(f32::INFINITY)
                    .desired_rows(10),
            );
        });

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !raw_input.trim().is_empty(),
                egui::Button::new("Verify & Save Credentials"),
            )
            .clicked()
        {
            let _ = cmd_tx.send(WorkerCmd::ImportAuth(raw_input.clone()));
        }
        if ui.button("Clear").clicked() {
            raw_input.clear();
        }
    });
}
