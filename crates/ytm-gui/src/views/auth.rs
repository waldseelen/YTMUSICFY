use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::SessionStatus;

use crate::state::{AsyncState, BrowserKind, WorkerCmd};
use crate::views::async_banner;

pub fn show(
    ui: &mut egui::Ui,
    session: &AsyncState<SessionStatus>,
    raw_input: &mut String,
    browser: &mut BrowserKind,
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
    ui.horizontal(|ui| {
        ui.strong("Auto-Import from Browser:");
        egui::ComboBox::from_id_salt("auth_browser_combo")
            .selected_text(browser.label())
            .show_ui(ui, |ui| {
                for kind in BrowserKind::ALL {
                    ui.selectable_value(browser, kind, kind.label());
                }
            });
        if ui.button("Import Session from Browser").clicked() {
            let _ = cmd_tx.send(WorkerCmd::ImportBrowserAuth(*browser));
        }
    });

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
        if ui.button("Import from File...").clicked() {
            let trimmed = raw_input.trim();
            if !trimmed.is_empty()
                && let Ok(contents) = std::fs::read_to_string(trimmed)
            {
                *raw_input = contents.clone();
                let _ = cmd_tx.send(WorkerCmd::ImportAuth(contents));
            }
        }
    });
    ui.small("Tip: To avoid clipboard freezes on Wayland, you can also paste a file path (e.g. /tmp/headers.txt) above and click 'Import from File...'.");
}
