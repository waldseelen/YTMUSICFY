use std::sync::mpsc::Sender;
use std::time::Instant;

use eframe::egui;
use ytm_core::{SearchFilter, SessionStatus};

use crate::bar_layout::TopBarLayout;
use crate::contracts::ThemeMode;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, paint_icon};
use crate::interaction::search_box_with_clear;
use crate::navigation::NavHistory;
use crate::search_controller::SearchController;
use crate::state::{AsyncState, BrowserKind, GuiAppConfig, GuiTab, WorkerCmd};
use crate::status_bus::StatusBus;
use crate::status_ribbon::{RibbonTiming, show_status_ribbon};
use crate::sync_queue::SyncQueue;
use crate::theme::Palette;

pub struct TopBarCtx<'a> {
    pub lang: ResolvedLang,
    pub palette: &'a Palette,
    pub session: &'a AsyncState<SessionStatus>,
    pub auth_browser: BrowserKind,
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

pub fn show_top_bar(ui: &mut egui::Ui, mut ctx: TopBarCtx<'_>) -> egui::Rect {
    let layout = TopBarLayout::from_width(ui.available_width(), ctx.gui_config.font_scale);
    let mut search_bar_rect = egui::Rect::NOTHING;

    ui.horizontal(|ui| {
        if layout.center_search {
            ui.allocate_ui_with_layout(
                egui::vec2(layout.side_width, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(layout.side_width);
                    ui.set_max_width(layout.side_width);
                    render_left_brand(ui, &mut ctx, &layout);
                },
            );
            let center_w = layout.search_width + 26.0;
            ui.allocate_ui_with_layout(
                egui::vec2(center_w, ui.available_height()),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(center_w);
                    ui.set_max_width(center_w);
                    search_bar_rect = render_search_slot(ui, &mut ctx, &layout);
                },
            );
            ui.allocate_ui_with_layout(
                egui::vec2(layout.side_width, ui.available_height()),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.set_min_width(layout.side_width);
                    ui.set_max_width(layout.side_width);
                    render_right_controls(ui, &mut ctx, &layout);
                },
            );
        } else {
            render_left_brand(ui, &mut ctx, &layout);
            search_bar_rect = render_search_slot(ui, &mut ctx, &layout);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                render_right_controls(ui, &mut ctx, &layout);
            });
        }
    });
    search_bar_rect
}

fn render_left_brand(ui: &mut egui::Ui, ctx: &mut TopBarCtx<'_>, layout: &TopBarLayout) {
    if icon_button(
        ui,
        Icon::ArrowLeft,
        16.0,
        ctx.nav_history.can_back(),
        tr(ctx.lang, Key::Back),
    )
    .clicked()
        && let Some(entry) = ctx.nav_history.go_back()
    {
        *ctx.active_tab = entry.tab;
        *ctx.selected_playlist_id = entry.playlist_id;
    }
    if icon_button(
        ui,
        Icon::ArrowRight,
        16.0,
        ctx.nav_history.can_forward(),
        tr(ctx.lang, Key::Forward),
    )
    .clicked()
        && let Some(entry) = ctx.nav_history.go_forward()
    {
        *ctx.active_tab = entry.tab;
        *ctx.selected_playlist_id = entry.playlist_id;
    }
    if let Some(tex) = crate::brand::logo_texture_for_mode(ui.ctx(), ctx.palette.dark_mode) {
        ui.add(egui::Image::new((tex.id(), egui::vec2(28.0, 28.0))).corner_radius(6.0));
    }
    if layout.show_brand_title {
        ui.strong(crate::brand::APP_NAME);
    }
}

fn render_search_slot(
    ui: &mut egui::Ui,
    ctx: &mut TopBarCtx<'_>,
    layout: &TopBarLayout,
) -> egui::Rect {
    let mut query_buf = ctx.search_controller.query.clone();
    let search_resp = ui.scope(|ui| {
        paint_icon(ui, Icon::Search, 14.0, ctx.palette.text_muted);
        let out = search_box_with_clear(
            ui,
            "dalga_global_search_input",
            &mut query_buf,
            tr(ctx.lang, Key::SearchPlaceholder),
            layout.search_width,
            ctx.focus_search_once,
        );
        if query_buf != ctx.search_controller.query {
            ctx.search_controller
                .on_query_change(&query_buf, ctx.now_ms);
        }
        if ctx.focus_search_once {
            ctx.search_controller.dropdown_open = true;
        }
        if out.submitted
            && !query_buf.trim().is_empty()
            && ctx.search_controller.highlighted_idx.is_none()
        {
            let q = query_buf.trim().to_string();
            ctx.gui_config.recent_searches = ctx.search_controller.record_recent(&q);
            ctx.search_controller.dropdown_open = false;
            *ctx.catalog_query = q.clone();
            *ctx.active_tab = GuiTab::Search;
            let _ = ctx.cmd_tx.send(WorkerCmd::SearchCatalog {
                query: q,
                filter: ctx.catalog_filter,
                continuation: None,
            });
        }
    });
    if search_resp.response.clicked()
        || ui
            .ctx()
            .memory(|m| m.has_focus(egui::Id::new("dalga_global_search_input")))
    {
        ctx.search_controller.dropdown_open = true;
    }
    let due = ctx.search_controller.poll_due(ctx.now_ms);
    if let Some((req_id, query)) = due.fetch_suggestions {
        let _ = ctx
            .cmd_tx
            .send(WorkerCmd::FetchSearchSuggestions { req_id, query });
    }
    if let Some((req_id, query)) = due.fetch_live {
        let _ = ctx
            .cmd_tx
            .send(WorkerCmd::SearchInstantLive { req_id, query });
    }
    search_resp.response.rect
}

fn render_right_controls(ui: &mut egui::Ui, ctx: &mut TopBarCtx<'_>, layout: &TopBarLayout) {
    let (profile_icon, profile_tip) = match ctx.session {
        AsyncState::Ready(s) => (
            Icon::Settings,
            format!(
                "{} ({}) — {}",
                s.account_name.as_deref().unwrap_or("Signed in"),
                s.channel_handle.as_deref().unwrap_or("-"),
                tr(ctx.lang, Key::SidebarSettings)
            ),
        ),
        AsyncState::Failed(_) => (
            Icon::User,
            format!("{} — Auth Settings", tr(ctx.lang, Key::AuthNotSignedIn)),
        ),
        _ => (
            Icon::Settings,
            tr(ctx.lang, Key::SidebarSettings).to_string(),
        ),
    };
    if icon_button(ui, profile_icon, 15.0, true, &profile_tip).clicked() {
        *ctx.active_tab = if matches!(ctx.session, AsyncState::Failed(_)) {
            GuiTab::Auth
        } else {
            GuiTab::Settings
        };
    }

    let sync_len = ctx.sync_queue.entries().len();
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
    let fill = if *ctx.sync_drawer_open {
        ctx.palette.accent_primary
    } else if sync_len > 0 {
        egui::Color32::from_rgb(92, 66, 128)
    } else {
        ctx.palette.bg_faint
    };
    if ui
        .add_enabled(
            true,
            egui::Button::image_and_text(
                Icon::Sync.image(14.0, ctx.palette.text_primary),
                egui::RichText::new(drawer_label)
                    .color(ctx.palette.text_primary)
                    .strong(),
            )
            .fill(fill)
            .corner_radius(5.0),
        )
        .on_hover_text("Toggle persistent Sync Staging Drawer (Ctrl+Shift+S)")
        .clicked()
    {
        *ctx.sync_drawer_open = !*ctx.sync_drawer_open;
    }

    if matches!(ctx.session, AsyncState::Failed(_))
        && ui
            .small_button("Import Session")
            .on_hover_text("One-click browser cookie import")
            .clicked()
    {
        let _ = ctx
            .cmd_tx
            .send(WorkerCmd::ImportBrowserAuth(ctx.auth_browser));
    }

    if icon_button(
        ui,
        Icon::MiniPlayer,
        15.0,
        true,
        tr(ctx.lang, Key::MiniPlayerToggle),
    )
    .clicked()
    {
        *ctx.mini_player_open = !*ctx.mini_player_open;
    }

    let (theme_icon, theme_tip) = match ctx.gui_config.theme_mode {
        ThemeMode::System => (Icon::Monitor, tr(ctx.lang, Key::ThemeSystem)),
        ThemeMode::Dark => (Icon::Moon, tr(ctx.lang, Key::ThemeDark)),
        ThemeMode::Light => (Icon::Sun, tr(ctx.lang, Key::ThemeLight)),
    };
    if icon_button(
        ui,
        theme_icon,
        15.0,
        true,
        &format!("{}: {theme_tip}", tr(ctx.lang, Key::ThemeModeLabel)),
    )
    .clicked()
    {
        ctx.gui_config.theme_mode = match ctx.gui_config.theme_mode {
            ThemeMode::System => ThemeMode::Dark,
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::System,
        };
        let _ = ctx
            .cmd_tx
            .send(WorkerCmd::UpdateGuiConfig(ctx.gui_config.clone()));
    }

    if (*ctx.ui_scale - 1.0).abs() > 0.01
        && ui
            .small_button(format!("{}%", (*ctx.ui_scale * 100.0).round()))
            .on_hover_text("Reset UI zoom (Ctrl+0)")
            .clicked()
    {
        *ctx.ui_scale = 1.0;
    }

    if layout.show_ribbon && ui.available_width() > 140.0 {
        let elapsed = ctx.start_instant.elapsed().as_secs_f32();
        show_status_ribbon(
            ui,
            ctx.status_bus,
            ctx.palette,
            ctx.lang,
            RibbonTiming {
                now_ms: ctx.now_ms,
                elapsed_secs: elapsed,
                ticker_speed: ctx.gui_config.ticker_speed,
                reduced_motion: ctx.gui_config.reduced_motion,
            },
            ctx.sync_queue,
            ctx.active_tab,
        );
    }
}
