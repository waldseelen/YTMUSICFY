use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::SessionStatus;

use crate::auth_store::AudioQuality;
use crate::browser_launch::SessionRefreshState;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_text_button};
use crate::state::{AsyncState, BrowserKind, WorkerCmd};
use crate::views::async_banner;

pub struct AuthAudioConfig<'a> {
    pub quality: &'a mut AudioQuality,
    pub cookie_bridge_active: bool,
    pub pending_quality: Option<AudioQuality>,
    pub refresh_state: &'a SessionRefreshState,
}

pub fn show(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    session: &AsyncState<SessionStatus>,
    raw_input: &mut String,
    browser: &mut BrowserKind,
    audio: AuthAudioConfig<'_>,
    cmd_tx: &Sender<WorkerCmd>,
) -> (bool, bool) {
    let mut trigger_refresh = false;
    ui.heading(tr(lang, Key::AuthTitle));
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
        if ui.button(tr(lang, Key::AuthImportBrowser)).clicked() {
            let _ = cmd_tx.send(WorkerCmd::ImportBrowserAuth(*browser));
        }
        if icon_text_button(
            ui,
            Icon::Refresh,
            tr(lang, Key::AuthRefreshSession),
            13.0,
            !audio.refresh_state.is_busy(),
        )
        .clicked()
        {
            trigger_refresh = true;
        }
    });
    match audio.refresh_state {
        SessionRefreshState::WaitingToImport { .. } | SessionRefreshState::Opening => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(tr(lang, Key::AuthRefreshingWait));
            });
        }
        SessionRefreshState::Importing => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Importing cookies...");
            });
        }
        SessionRefreshState::Failed(err) => {
            ui.colored_label(egui::Color32::from_rgb(235, 95, 95), err);
        }
        _ => {}
    }

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.strong(tr(lang, Key::AuthAudioQuality));
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
        "DevTools Fallback: On music.youtube.com press F12 -> Network -> click Home or F5 -> filter 'youtubei/v1' -> Copy Request Headers:",
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
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(tr(lang, Key::AuthImportClipboard))
            .on_hover_text("Reads system clipboard off the UI thread via wl-paste/xclip/PowerShell without freezing Wayland.")
            .clicked()
        {
            ui.ctx().memory_mut(|m| m.data.remove::<String>(err_id));
            let _ = cmd_tx.send(WorkerCmd::ImportClipboardAuth);
        }
        if ui
            .add_enabled(
                !raw_input.trim().is_empty(),
                egui::Button::new(tr(lang, Key::AuthVerifySave)),
            )
            .clicked()
        {
            ui.ctx().memory_mut(|m| m.data.remove::<String>(err_id));
            let _ = cmd_tx.send(WorkerCmd::ImportAuth(raw_input.clone()));
        }
        if ui.button(tr(lang, Key::AuthClear)).clicked() {
            raw_input.clear();
            ui.ctx().memory_mut(|m| m.data.remove::<String>(err_id));
        }
        if ui.button(tr(lang, Key::AuthImportFile)).clicked() {
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
    (apply_now, trigger_refresh)
}
