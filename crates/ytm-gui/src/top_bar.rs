use std::sync::mpsc::Sender;
use std::time::Instant;

use eframe::egui;
use ytm_core::{SearchFilter, SessionStatus};

use crate::contracts::{AccentPalette, ThemeMode};
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, paint_icon};
use crate::interaction::search_box_with_clear;
use crate::navigation::NavHistory;
use crate::search_controller::SearchController;
use crate::state::{AsyncState, GuiAppConfig, GuiTab, WorkerCmd};
use crate::status_bus::StatusBus;
use crate::status_ribbon::{RibbonTiming, show_status_ribbon};
use crate::sync_queue::SyncQueue;
use crate::theme::Palette;

pub struct TopBarCtx<'a> {
    pub lang: ResolvedLang,
    pub palette: &'a Palette,
    pub session: &'a AsyncState<SessionStatus>,
    pub active_tab: &'a mut GuiTab,
    pub selected_playlist_id: &'a mut Option<String>,
    pub nav_history: &'a mut NavHistory,
    pub ui_scale: &'a mut f32,
    pub gui_config: &'a mut GuiAppConfig,
    pub search_controller: &'a mut SearchController,
    pub catalog_query: &'a mut String,
    pub catalog_filter: SearchFilter,
    pub focus_search_once: bool,
    pub status_bus: &'a mut StatusBus,
    pub sync_queue: &'a mut SyncQueue,
    pub sync_drawer_open: &'a mut bool,
    pub mini_player_open: &'a mut bool,
    pub start_instant: Instant,
    pub now_ms: u64,
    pub cmd_tx: &'a Sender<WorkerCmd>,
}

pub fn show_top_bar(ui: &mut egui::Ui, ctx: TopBarCtx<'_>) -> egui::Rect {
    let TopBarCtx {
        lang,
        palette,
        session,
        active_tab,
        selected_playlist_id,
        nav_history,
        ui_scale,
        gui_config,
        search_controller,
        catalog_query,
        catalog_filter,
        focus_search_once,
        status_bus,
        sync_queue,
        sync_drawer_open,
        mini_player_open,
        start_instant,
        now_ms,
        cmd_tx,
    } = ctx;
    let layout =
        crate::bar_layout::TopBarLayout::from_width(ui.available_width(), gui_config.font_scale);
    let mut search_bar_rect = egui::Rect::NOTHING;
    ui.horizontal(|ui| {
        if icon_button(
            ui,
            Icon::ArrowLeft,
            16.0,
            nav_history.can_back(),
            tr(lang, Key::Back),
        )
        .clicked()
            && let Some(entry) = nav_history.go_back()
        {
            *active_tab = entry.tab;
            *selected_playlist_id = entry.playlist_id;
        }
        if icon_button(
            ui,
            Icon::ArrowRight,
            16.0,
            nav_history.can_forward(),
            tr(lang, Key::Forward),
        )
        .clicked()
            && let Some(entry) = nav_history.go_forward()
        {
            *active_tab = entry.tab;
            *selected_playlist_id = entry.playlist_id;
        }
        if let Some(tex) = crate::brand::logo_texture(ui.ctx()) {
            ui.add(egui::Image::new((tex.id(), egui::vec2(28.0, 28.0))).corner_radius(6.0));
        }
        if layout.show_brand_title {
            ui.strong(crate::brand::APP_NAME);
        }
        ui.separator();

        let search_w = layout.search_width;
        let mut query_buf = search_controller.query.clone();
        let search_resp = ui.scope(|ui| {
            paint_icon(ui, Icon::Search, 14.0, palette.text_muted);
            let out = search_box_with_clear(
                ui,
                "dalga_global_search_input",
                &mut query_buf,
                tr(lang, Key::SearchPlaceholder),
                search_w,
                focus_search_once,
            );
            if query_buf != search_controller.query {
                search_controller.on_query_change(&query_buf, now_ms);
            }
            if focus_search_once {
                search_controller.dropdown_open = true;
            }
            if out.submitted
                && !query_buf.trim().is_empty()
                && search_controller.highlighted_idx.is_none()
            {
                let q = query_buf.trim().to_string();
                gui_config.recent_searches = search_controller.record_recent(&q);
                search_controller.dropdown_open = false;
                *catalog_query = q.clone();
                *active_tab = GuiTab::Search;
                let _ = cmd_tx.send(WorkerCmd::SearchCatalog {
                    query: q,
                    filter: catalog_filter,
                    continuation: None,
                });
            }
        });
        search_bar_rect = search_resp.response.rect;
        if search_resp.response.clicked()
            || ui
                .ctx()
                .memory(|m| m.has_focus(egui::Id::new("dalga_global_search_input")))
        {
            search_controller.dropdown_open = true;
        }
        let due = search_controller.poll_due(now_ms);
        if let Some((req_id, query)) = due.fetch_suggestions {
            let _ = cmd_tx.send(WorkerCmd::FetchSearchSuggestions { req_id, query });
        }
        if let Some((req_id, query)) = due.fetch_live {
            let _ = cmd_tx.send(WorkerCmd::SearchInstantLive { req_id, query });
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (profile_icon, profile_tip) = match session {
                AsyncState::Ready(s) => (
                    Icon::Settings,
                    format!(
                        "{} ({}) — {}",
                        s.account_name.as_deref().unwrap_or("Signed in"),
                        s.channel_handle.as_deref().unwrap_or("-"),
                        tr(lang, Key::SidebarSettings)
                    ),
                ),
                AsyncState::Failed(_) => (Icon::User, tr(lang, Key::AuthNotSignedIn).to_string()),
                _ => (Icon::Settings, tr(lang, Key::SidebarSettings).to_string()),
            };
            if icon_button(ui, profile_icon, 15.0, true, &profile_tip).clicked() {
                *active_tab = if matches!(session, AsyncState::Failed(_)) {
                    GuiTab::Auth
                } else {
                    GuiTab::Settings
                };
            }

            let sync_len = sync_queue.entries().len();
            let drawer_label = if !layout.show_brand_title {
                if sync_len > 0 {
                    format!("{sync_len}")
                } else {
                    String::new()
                }
            } else if sync_len > 0 {
                format!("Sync ({sync_len})")
            } else {
                "Sync".to_string()
            };
            let fill = if *sync_drawer_open {
                palette.accent_primary
            } else if sync_len > 0 {
                egui::Color32::from_rgb(92, 66, 128)
            } else {
                palette.bg_faint
            };
            if ui
                .add_enabled(
                    true,
                    egui::Button::image_and_text(
                        crate::icons::Icon::Sync.image(14.0, palette.text_primary),
                        egui::RichText::new(drawer_label)
                            .color(palette.text_primary)
                            .strong(),
                    )
                    .fill(fill)
                    .corner_radius(5.0),
                )
                .on_hover_text("Toggle persistent Sync Staging Drawer (Ctrl+Shift+S)")
                .clicked()
            {
                *sync_drawer_open = !*sync_drawer_open;
            }

            if matches!(session, AsyncState::Failed(_))
                && ui
                    .small_button(tr(lang, Key::SidebarAuth))
                    .on_hover_text(tr(lang, Key::AuthNotSignedIn))
                    .clicked()
            {
                *active_tab = GuiTab::Auth;
            }

            if icon_button(
                ui,
                Icon::MiniPlayer,
                15.0,
                true,
                tr(lang, Key::MiniPlayerToggle),
            )
            .clicked()
            {
                *mini_player_open = !*mini_player_open;
            }

            let (theme_icon, theme_tip) = match gui_config.theme_mode {
                ThemeMode::System => (Icon::Monitor, tr(lang, Key::ThemeSystem)),
                ThemeMode::Dark => (Icon::Moon, tr(lang, Key::ThemeDark)),
                ThemeMode::Light => (Icon::Sun, tr(lang, Key::ThemeLight)),
            };
            if icon_button(
                ui,
                theme_icon,
                15.0,
                true,
                &format!("{}: {theme_tip}", tr(lang, Key::ThemeModeLabel)),
            )
            .clicked()
            {
                gui_config.theme_mode = match gui_config.theme_mode {
                    ThemeMode::System => ThemeMode::Dark,
                    ThemeMode::Dark => ThemeMode::Light,
                    ThemeMode::Light => ThemeMode::System,
                };
                let _ = cmd_tx.send(WorkerCmd::UpdateGuiConfig(gui_config.clone()));
            }
            let accent_label = match gui_config.accent_palette {
                AccentPalette::Emerald => tr(lang, Key::AccentEmerald),
                AccentPalette::Gold => tr(lang, Key::AccentGold),
                AccentPalette::YtmPink => tr(lang, Key::AccentYtmPink),
            };
            let accent_btn = if layout.show_accent_text {
                ui.small_button(accent_label)
            } else {
                ui.small_button(egui::RichText::new("●").color(palette.accent_primary))
            };
            if accent_btn
                .on_hover_text(format!("{}: {accent_label}", tr(lang, Key::AccentLabel)))
                .clicked()
            {
                gui_config.accent_palette = match gui_config.accent_palette {
                    AccentPalette::Emerald => AccentPalette::Gold,
                    AccentPalette::Gold => AccentPalette::YtmPink,
                    AccentPalette::YtmPink => AccentPalette::Emerald,
                };
                let _ = cmd_tx.send(WorkerCmd::UpdateGuiConfig(gui_config.clone()));
            }

            if (*ui_scale - 1.0).abs() > 0.01
                && ui
                    .small_button(format!("{}%", (*ui_scale * 100.0).round()))
                    .on_hover_text("Reset UI zoom (Ctrl+0)")
                    .clicked()
            {
                *ui_scale = 1.0;
            }

            if layout.show_ribbon && ui.available_width() > 140.0 {
                let elapsed = start_instant.elapsed().as_secs_f32();
                show_status_ribbon(
                    ui,
                    status_bus,
                    palette,
                    lang,
                    RibbonTiming {
                        now_ms,
                        elapsed_secs: elapsed,
                        ticker_speed: gui_config.ticker_speed,
                        reduced_motion: gui_config.reduced_motion,
                    },
                    sync_queue,
                    active_tab,
                );
            }
        });
    });
    search_bar_rect
}
