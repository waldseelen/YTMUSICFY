use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::SessionStatus;

use crate::auth_store::AudioQuality;
use crate::state::{AsyncState, BrowserKind, WorkerCmd};
use crate::views::async_banner;

pub struct AuthAudioConfig<'a> {
    pub quality: &'a mut AudioQuality,
    pub cookie_bridge_active: bool,
    pub pending_quality: Option<AudioQuality>,
}

pub fn show(
    ui: &mut egui::Ui,
    session: &AsyncState<SessionStatus>,
    raw_input: &mut String,
    browser: &mut BrowserKind,
    audio: AuthAudioConfig<'_>,
    cmd_tx: &Sender<WorkerCmd>,
) -> bool {
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
    ui.horizontal(|ui| {
        ui.strong("Audio Stream Quality:");
        let prev_quality = *audio.quality;
        egui::ComboBox::from_id_salt("audio_quality_combo")
            .selected_text(audio.quality.label())
            .show_ui(ui, |ui| {
                for preset in AudioQuality::ALL {
                    ui.selectable_value(audio.quality, preset, preset.label());
                }
            });
        if *audio.quality != prev_quality {
            let _ = cmd_tx.send(WorkerCmd::SetAudioQuality(*audio.quality));
        }
        if audio.cookie_bridge_active {
            ui.colored_label(
                egui::Color32::from_rgb(90, 200, 120),
                "Cookie Bridge: Active (0600 -> web_music/tv)",
            );
        } else {
            ui.weak("Cookie Bridge: Anonymous (android fallback)");
        }
    });
    let mut apply_now = false;
    if let Some(pq) = audio.pending_quality {
        ui.horizontal(|ui| {
            ui.colored_label(
                egui::Color32::from_rgb(235, 185, 65),
                format!(
                    "Pending stream config ({}) — applies on next track.",
                    pq.label()
                ),
            );
            if ui.button("Apply Now").clicked() {
                apply_now = true;
            }
        });
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

    let err_id = egui::Id::new("auth_file_err");
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !raw_input.trim().is_empty(),
                egui::Button::new("Verify & Save Credentials"),
            )
            .clicked()
        {
            ui.ctx().memory_mut(|m| m.data.remove::<String>(err_id));
            let _ = cmd_tx.send(WorkerCmd::ImportAuth(raw_input.clone()));
        }
        if ui.button("Clear").clicked() {
            raw_input.clear();
            ui.ctx().memory_mut(|m| m.data.remove::<String>(err_id));
        }
        if ui.button("Import from File...").clicked() {
            let trimmed = raw_input.trim();
            if trimmed.is_empty() {
                ui.ctx().memory_mut(|m| {
                    m.data.insert_temp(
                        err_id,
                        "Enter a file path in the box above before clicking 'Import from File...'."
                            .to_string(),
                    );
                });
            } else {
                match std::fs::read_to_string(trimmed) {
                    Ok(contents) => {
                        ui.ctx().memory_mut(|m| m.data.remove::<String>(err_id));
                        *raw_input = contents.clone();
                        let _ = cmd_tx.send(WorkerCmd::ImportAuth(contents));
                    }
                    Err(err) => {
                        ui.ctx().memory_mut(|m| {
                            m.data
                                .insert_temp(err_id, format!("Failed to read '{trimmed}': {err}"));
                        });
                    }
                }
            }
        }
    });
    if let Some(file_err) = ui.ctx().memory(|m| m.data.get_temp::<String>(err_id)) {
        ui.colored_label(egui::Color32::from_rgb(235, 90, 90), file_err);
    }
    ui.small("Tip: To avoid clipboard freezes on Wayland, you can also paste a file path (e.g. /tmp/headers.txt) above and click 'Import from File...'.");
    apply_now
}
