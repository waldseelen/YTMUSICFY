use std::sync::mpsc::Sender;

use eframe::egui;

use crate::contracts::{AccentPalette, ThemeMode, UiLang};
use crate::i18n::{Key, ResolvedLang, tr};
use crate::state::{GuiAppConfig, MutationPolicy, WorkerCmd};

/// Dedicated Settings panel editing `config.toml` governance, appearance, ribbon, and enrichment.
pub fn show(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    cfg: &mut GuiAppConfig,
    cmd_tx: &Sender<WorkerCmd>,
) {
    ui.heading(tr(lang, Key::SettingsTitle));
    ui.separator();

    egui::ScrollArea::vertical()
        .id_salt("settings_panel_scroll")
        .show(ui, |ui| {
            ui.strong(tr(lang, Key::SettingsRibbonHeading));
            ui.horizontal(|ui| {
                ui.label(format!("{}:", tr(lang, Key::SettingsLangLabel)));
                ui.radio_value(&mut cfg.ui_lang, UiLang::System, tr(lang, Key::LangSystem));
                ui.radio_value(&mut cfg.ui_lang, UiLang::En, tr(lang, Key::LangEnglish));
                ui.radio_value(&mut cfg.ui_lang, UiLang::Tr, tr(lang, Key::LangTurkish));
            });
            ui.horizontal(|ui| {
                ui.label(format!("{}:", tr(lang, Key::ThemeModeLabel)));
                ui.radio_value(
                    &mut cfg.theme_mode,
                    ThemeMode::System,
                    tr(lang, Key::ThemeSystem),
                );
                ui.radio_value(
                    &mut cfg.theme_mode,
                    ThemeMode::Dark,
                    tr(lang, Key::ThemeDark),
                );
                ui.radio_value(
                    &mut cfg.theme_mode,
                    ThemeMode::Light,
                    tr(lang, Key::ThemeLight),
                );
            });
            ui.horizontal(|ui| {
                ui.label(format!("{}:", tr(lang, Key::AccentLabel)));
                ui.radio_value(
                    &mut cfg.accent_palette,
                    AccentPalette::Emerald,
                    tr(lang, Key::AccentEmerald),
                );
                ui.radio_value(
                    &mut cfg.accent_palette,
                    AccentPalette::Gold,
                    tr(lang, Key::AccentGold),
                );
                ui.radio_value(
                    &mut cfg.accent_palette,
                    AccentPalette::YtmPink,
                    tr(lang, Key::AccentYtmPink),
                );
            });
            ui.checkbox(&mut cfg.dynamic_accent, tr(lang, Key::DynamicAccentLabel));
            ui.checkbox(
                &mut cfg.greetings_enabled,
                tr(lang, Key::SettingsGreetingsToggle),
            );
            ui.horizontal(|ui| {
                ui.label(tr(lang, Key::SettingsTickerSpeed));
                ui.add(egui::DragValue::new(&mut cfg.ticker_speed).range(16.0..=160.0));
            });
            ui.checkbox(
                &mut cfg.reduced_motion,
                tr(lang, Key::SettingsReducedMotion),
            );
            ui.horizontal(|ui| {
                ui.label(tr(lang, Key::SettingsBrowserLabel));
                let cur = cfg
                    .preferred_external_browser
                    .as_deref()
                    .unwrap_or(tr(lang, Key::SettingsBrowserDefault));
                egui::ComboBox::from_id_salt("settings_ext_browser")
                    .selected_text(cur)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(
                                cfg.preferred_external_browser.is_none(),
                                tr(lang, Key::SettingsBrowserDefault),
                            )
                            .clicked()
                        {
                            cfg.preferred_external_browser = None;
                        }
                        for b in ["brave", "firefox", "chromium", "google-chrome", "msedge"] {
                            if ui
                                .selectable_label(
                                    cfg.preferred_external_browser.as_deref() == Some(b),
                                    b,
                                )
                                .clicked()
                            {
                                cfg.preferred_external_browser = Some(b.to_string());
                            }
                        }
                    });
            });

            ui.add_space(10.0);
            ui.separator();
            ui.strong(tr(lang, Key::SettingsTrayHeading));
            ui.checkbox(&mut cfg.tray_enabled, tr(lang, Key::SettingsTrayEnabled));
            ui.add_enabled_ui(cfg.tray_enabled, |ui| {
                ui.checkbox(&mut cfg.close_to_tray, tr(lang, Key::SettingsCloseToTray));
            });
            ui.checkbox(
                &mut cfg.mini_player_opacity_on_idle,
                tr(lang, Key::MiniPlayerOpacityIdle),
            );

            ui.add_space(10.0);
            ui.separator();
            ui.strong(tr(lang, Key::SettingsSafetyHeading));
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut cfg.mutation_policy,
                    MutationPolicy::StrictLock,
                    tr(lang, Key::SettingsStrictLock),
                );
                ui.radio_value(
                    &mut cfg.mutation_policy,
                    MutationPolicy::QuickLike,
                    tr(lang, Key::SettingsQuickLike),
                );
            });
            ui.checkbox(
                &mut cfg.auto_refresh_browser_cookies,
                tr(lang, Key::SettingsAutoRefreshCookies),
            );

            ui.add_space(10.0);
            ui.separator();
            ui.strong(tr(lang, Key::SettingsProtocolHeading));
            ui.horizontal(|ui| {
                ui.label("hl:");
                ui.add(egui::TextEdit::singleline(&mut cfg.innertube.hl).desired_width(60.0));
                ui.label("gl:");
                if ui
                    .add(egui::TextEdit::singleline(&mut cfg.innertube.gl).desired_width(60.0))
                    .changed()
                {
                    cfg.innertube.gl = cfg.innertube.gl.to_ascii_uppercase();
                }
            });
            ui.horizontal(|ui| {
                ui.label("Chunk:");
                ui.add(egui::DragValue::new(&mut cfg.innertube.mutation_chunk_size).range(1..=50));
                ui.label("Delay (ms):");
                ui.add(egui::DragValue::new(&mut cfg.innertube.mutation_delay_ms).range(0..=5000));
                ui.label("Max pages:");
                ui.add(
                    egui::DragValue::new(&mut cfg.innertube.max_pagination_pages).range(5..=2000),
                );
            });

            ui.add_space(10.0);
            ui.separator();
            ui.strong(tr(lang, Key::SettingsEnrichHeading));
            ui.checkbox(
                &mut cfg.enrichment.artist_cache_first,
                tr(lang, Key::SettingsFastMode),
            );
            ui.checkbox(
                &mut cfg.enrichment.enable_deezer_fallback,
                tr(lang, Key::SettingsDeezerFallback),
            );
            ui.checkbox(
                &mut cfg.enrichment.enable_listenbrainz_fallback,
                tr(lang, Key::SettingsListenBrainzFallback),
            );
            ui.horizontal(|ui| {
                ui.label(tr(lang, Key::SettingsLastfmKey));
                let mut lfm = cfg.enrichment.lastfm_api_key.clone().unwrap_or_default();
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut lfm)
                            .password(true)
                            .desired_width(240.0),
                    )
                    .changed()
                {
                    cfg.enrichment.lastfm_api_key =
                        (!lfm.trim().is_empty()).then(|| lfm.trim().to_string());
                }
            });

            ui.add_space(12.0);
            if ui.button(tr(lang, Key::SettingsSave)).clicked() {
                let _ = cmd_tx.send(WorkerCmd::UpdateGuiConfig(cfg.clone()));
            }

            ui.add_space(14.0);
            ui.separator();
            crate::shortcuts::show_shortcuts_table(ui, lang);
        });
}
