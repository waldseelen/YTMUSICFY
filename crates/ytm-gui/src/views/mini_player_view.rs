use eframe::egui;

use crate::app::YtmGuiApp;
use crate::contracts::{MiniPlayerGeometry, MiniPlayerMode};
use crate::i18n::{Key, tr};
use crate::icons::{Icon, icon_button, paint_icon};
use crate::mini_player::{
    MINI_COMPACT_HEIGHT, MINI_EDGE_MARGIN, MINI_EXPANDED_HEIGHT, MINI_MIN_HEIGHT, MINI_MIN_WIDTH,
    MINI_SNAP_THRESHOLD, default_anchor_pos, effective_idle_alpha, monitor_key,
    snap_to_monitor_edges,
};
use crate::playback_ops::{PlayAction, apply_player_bar_outcome};
use crate::player::PlayerState;
use crate::state::WorkerCmd;
use crate::views::inspect_helpers::format_duration;
use crate::views::mini_player_search::render_mini_expanded_search;
use crate::views::player_bar::PlayerBarOutcome;
use crate::window_lifecycle::set_main_window_visible;

/// Render the borderless, always-on-top Mini Player secondary viewport when open.
pub(crate) fn show_mini_player_viewport(ctx: &egui::Context, app: &mut YtmGuiApp) {
    if !app.mini_player.open {
        return;
    }
    #[cfg(feature = "screenshot-harness")]
    if std::env::var_os("DALGA_SCREENSHOT_OUT").is_some() {
        ctx.set_embed_viewports(true);
    }
    let vp_id = egui::ViewportId::from_hash_of("dalga_mini_player");
    let mkey = app
        .mini_player
        .last_monitor_key
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let saved = app.gui_config.mini_player_layouts.get(&mkey).copied();
    let desired_size = app.mini_player.desired_size(saved.as_ref());

    let mut builder = egui::ViewportBuilder::default()
        .with_title("Dalga Mini")
        .with_app_id("ytm-architect-mini")
        .with_decorations(false)
        .with_resizable(true)
        .with_always_on_top()
        .with_inner_size(desired_size)
        .with_min_inner_size([MINI_MIN_WIDTH, MINI_MIN_HEIGHT]);
    if let Some(geom) = saved {
        builder = builder.with_position([geom.x, geom.y]);
    }
    if let Some(icon) = crate::brand::load_window_icon() {
        builder = builder.with_icon(std::sync::Arc::new(icon));
    }

    ctx.show_viewport_immediate(vp_id, builder, |mini_ctx, class| {
        if class == egui::ViewportClass::Embedded {
            let mut open = app.mini_player.open;
            let pal = app.theme_state.palette;
            let win_frame = egui::Frame::window(&mini_ctx.style())
                .fill(pal.bg_panel)
                .stroke(egui::Stroke::new(1.0_f32, pal.border_subtle))
                .inner_margin(10.0);
            egui::Window::new("Dalga Mini")
                .open(&mut open)
                .title_bar(false)
                .frame(win_frame)
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::RIGHT_BOTTOM, [-18.0, -78.0])
                .default_size(desired_size)
                .show(mini_ctx, |ui| {
                    render_mini_contents(ui, mini_ctx, app, false);
                });
            app.mini_player.open = open;
            return;
        }

        if mini_ctx.input(|i| i.viewport().close_requested()) {
            app.mini_player.open = false;
            return;
        }

        let mon_size = mini_ctx.input(|i| i.viewport().monitor_size);
        let cur_key = monitor_key(mon_size);
        app.mini_player.last_monitor_key = Some(cur_key.clone());

        if app.mini_player.applied_geometry_for_key.as_deref() != Some(&cur_key) {
            app.mini_player.applied_geometry_for_key = Some(cur_key.clone());
            if let Some(geom) = app.gui_config.mini_player_layouts.get(&cur_key).copied() {
                let target = if let Some(msz) = mon_size {
                    snap_to_monitor_edges(
                        egui::pos2(geom.x, geom.y),
                        egui::vec2(geom.width, geom.height),
                        msz,
                        MINI_SNAP_THRESHOLD,
                        MINI_EDGE_MARGIN,
                    )
                } else {
                    egui::pos2(geom.x, geom.y)
                };
                mini_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target));
            } else if let Some(msz) = mon_size {
                let anchor = default_anchor_pos(desired_size, msz);
                mini_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(anchor));
            }
        }

        let (hovered, focused, pointer_down, outer_rect) = mini_ctx.input(|i| {
            (
                i.pointer.has_pointer(),
                i.viewport().focused == Some(true),
                i.pointer.primary_down(),
                i.viewport().outer_rect,
            )
        });
        let alpha =
            effective_idle_alpha(app.gui_config.mini_player_opacity_on_idle, hovered, focused);
        let pal = app.theme_state.palette;
        let frame = egui::Frame::central_panel(&mini_ctx.style())
            .fill(pal.bg_panel.gamma_multiply(alpha))
            .stroke(egui::Stroke::new(1.0_f32, pal.border_subtle))
            .inner_margin(10.0);

        egui::CentralPanel::default()
            .frame(frame)
            .show(mini_ctx, |ui| {
                render_mini_contents(ui, mini_ctx, app, true);
            });

        #[cfg(feature = "screenshot-harness")]
        if crate::screenshot_seed::maybe_capture_screenshot(mini_ctx, ctx) {
            app.explicit_quit = true;
        }

        if app.mini_player.was_pointer_down
            && !pointer_down
            && let (Some(rect), Some(msz)) = (outer_rect, mon_size)
        {
            let snapped = snap_to_monitor_edges(
                rect.min,
                rect.size(),
                msz,
                MINI_SNAP_THRESHOLD,
                MINI_EDGE_MARGIN,
            );
            if (snapped - rect.min).length() > 0.5 {
                mini_ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(snapped));
            }
            persist_mini_layout(app, &cur_key, snapped, rect.size());
        }
        app.mini_player.was_pointer_down = pointer_down;
    });
}

fn persist_mini_layout(app: &mut YtmGuiApp, key: &str, pos: egui::Pos2, size: egui::Vec2) {
    let next = MiniPlayerGeometry {
        x: pos.x,
        y: pos.y,
        width: size.x.max(MINI_MIN_WIDTH),
        height: size.y.max(MINI_MIN_HEIGHT),
        mode: app.mini_player.mode,
    };
    let changed = app
        .gui_config
        .mini_player_layouts
        .get(key)
        .is_none_or(|prev| {
            (prev.x - next.x).abs() > 1.0
                || (prev.y - next.y).abs() > 1.0
                || (prev.width - next.width).abs() > 1.0
                || (prev.height - next.height).abs() > 1.0
                || prev.mode != next.mode
        });
    if changed {
        app.gui_config
            .mini_player_layouts
            .insert(key.to_string(), next);
        let _ = app
            .cmd_tx
            .send(WorkerCmd::UpdateGuiConfig(app.gui_config.clone()));
    }
}

fn render_mini_contents(
    ui: &mut egui::Ui,
    mini_ctx: &egui::Context,
    app: &mut YtmGuiApp,
    native_chrome: bool,
) {
    let lang = app.lang();
    let pal = app.theme_state.palette;
    let mut bar_out = PlayerBarOutcome::default();

    // Header drag strip + mode toggle + restore main + close
    ui.horizontal(|ui| {
        let drag_resp = ui
            .horizontal(|ui| {
                paint_icon(ui, Icon::DragHandle, 13.0, pal.text_muted);
                ui.strong(crate::brand::APP_NAME);
            })
            .response
            .interact(egui::Sense::drag());
        if native_chrome && drag_resp.drag_started() {
            mini_ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icon_button(ui, Icon::Close, 13.0, true, tr(lang, Key::DrawerClose)).clicked() {
                app.mini_player.open = false;
            }
            if icon_button(
                ui,
                Icon::ExternalLink,
                13.0,
                true,
                tr(lang, Key::MiniPlayerRestoreMain),
            )
            .clicked()
            {
                set_main_window_visible(&ui.ctx().clone(), app, true);
            }
            let (exp_icon, exp_tip) = match app.mini_player.mode {
                MiniPlayerMode::Compact => (Icon::Search, tr(lang, Key::MiniPlayerExpand)),
                MiniPlayerMode::Expanded => (Icon::ChevronUp, tr(lang, Key::MiniPlayerCollapse)),
            };
            if icon_button(ui, exp_icon, 13.0, true, exp_tip).clicked() {
                app.mini_player.mode = match app.mini_player.mode {
                    MiniPlayerMode::Compact => MiniPlayerMode::Expanded,
                    MiniPlayerMode::Expanded => MiniPlayerMode::Compact,
                };
                if native_chrome {
                    let h = match app.mini_player.mode {
                        MiniPlayerMode::Compact => MINI_COMPACT_HEIGHT,
                        MiniPlayerMode::Expanded => MINI_EXPANDED_HEIGHT,
                    };
                    mini_ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        ui.available_width().max(MINI_MIN_WIDTH) + 20.0,
                        h,
                    )));
                }
            }
        });
    });

    // Track info + like + cover
    let snap = app.player.snapshot();
    let current = app.play_queue.current_track().cloned();
    let art_tex = current
        .as_ref()
        .and_then(|t| t.effective_thumbnail_url())
        .and_then(|url| app.artwork.texture_for(mini_ctx, &url).cloned());
    ui.horizontal(|ui| {
        if let Some(tex) = &art_tex {
            ui.add(egui::Image::new((tex.id(), egui::vec2(44.0, 44.0))).corner_radius(4.0));
        } else {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 4.0, ui.visuals().widgets.inactive.bg_fill);
            paint_icon(ui, Icon::Music, 18.0, pal.text_muted);
        }
        ui.vertical(|ui| {
            if let Some(track) = &current {
                ui.add(egui::Label::new(egui::RichText::new(&track.title).strong()).truncate());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(track.primary_artist_name().unwrap_or("-")).small(),
                    )
                    .truncate(),
                );
            } else {
                ui.strong(tr(lang, Key::PlayerNotPlaying));
                ui.small(tr(lang, Key::PlayerPlayHint));
            }
        });
    });

    // Transport + Like + Shuffle + Repeat
    let has_current = current.is_some();
    ui.horizontal(|ui| {
        if let Some(track) = &current {
            let liked = app.play_queue.is_liked(&track.video_id);
            let heart_col = if liked {
                egui::Color32::from_rgb(235, 85, 115)
            } else {
                ui.visuals().weak_text_color()
            };
            if ui
                .add(egui::Button::image(Icon::Heart.image(14.0, heart_col)))
                .on_hover_text(tr(lang, Key::PlayerLike))
                .clicked()
            {
                bar_out.actions.push(PlayAction::ToggleLike(track.clone()));
            }
        }
        if ui
            .add(
                egui::Button::image(Icon::Shuffle.image(14.0, ui.visuals().text_color()))
                    .selected(app.play_queue.is_shuffle()),
            )
            .on_hover_text(tr(lang, Key::PlayerShuffle))
            .clicked()
        {
            bar_out.toggle_shuffle = true;
        }
        if icon_button(ui, Icon::Prev, 14.0, has_current, tr(lang, Key::PlayerPrev)).clicked() {
            bar_out.previous = true;
        }
        let play_icon = match &snap.state {
            PlayerState::Playing { .. } => Icon::Pause,
            PlayerState::Loading { .. } => Icon::Refresh,
            _ => Icon::Play,
        };
        if icon_button(
            ui,
            play_icon,
            15.0,
            has_current,
            tr(lang, Key::ShortcutPlayPause),
        )
        .clicked()
        {
            bar_out.toggle_pause = true;
        }
        if icon_button(ui, Icon::Next, 14.0, has_current, tr(lang, Key::PlayerNext)).clicked() {
            bar_out.next = true;
        }
        let rep = app.play_queue.repeat_mode();
        if icon_button(ui, rep.icon_kind(), 14.0, true, tr(lang, Key::PlayerRepeat)).clicked() {
            bar_out.cycle_repeat = true;
        }
    });

    // Seekable progress row + resize grip
    ui.horizontal(|ui| {
        let max_dur = snap.duration_sec.max(1.0) as f32;
        let mut pos = (snap.position_sec as f32).clamp(0.0, max_dur);
        ui.monospace(format_duration(snap.position_sec.max(0.0) as u64));
        let slider = ui.add_enabled(
            has_current && snap.duration_sec > 0.0,
            egui::Slider::new(&mut pos, 0.0..=max_dur)
                .show_value(false)
                .trailing_fill(true),
        );
        if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
            bar_out.seek_to = Some(f64::from(pos));
        }
        ui.monospace(format_duration(snap.duration_sec.max(0.0) as u64));
        if native_chrome {
            let grip = ui
                .add(egui::Label::new(egui::RichText::new("..").small()).sense(egui::Sense::drag()))
                .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
            if grip.drag_started() {
                mini_ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(
                    egui::ResizeDirection::SouthEast,
                ));
            }
        }
    });

    // Expanded search reusing app.search_controller
    if app.mini_player.mode == MiniPlayerMode::Expanded {
        ui.separator();
        render_mini_expanded_search(ui, app, lang, &mut bar_out.actions);
    }

    apply_player_bar_outcome(bar_out, app);
}
