use eframe::egui::{self, Color32};

use crate::contracts::{StatusMessage, StatusPriority};
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, paint_icon};
use crate::state::GuiTab;
use crate::status_bus::StatusBus;
use crate::sync_queue::SyncQueue;
use crate::theme::Palette;

/// Compute right-to-left horizontal scroll offset in pixels using an injected clock.
/// Returns `0.0` when `reduced_motion` is enabled or text already fits inside `viewport_width`.
#[must_use]
pub fn scroll_offset(
    elapsed_secs: f32,
    text_width: f32,
    viewport_width: f32,
    speed_px_per_sec: f32,
    reduced_motion: bool,
) -> f32 {
    if reduced_motion || text_width <= viewport_width || speed_px_per_sec <= 0.0 {
        return 0.0;
    }
    let gap = 36.0;
    let cycle = text_width + viewport_width + gap;
    let traveled = (elapsed_secs.max(0.0) * speed_px_per_sec) % cycle;
    viewport_width - traveled
}

/// Actions requested from the Status Ribbon or its 50-message history popup.
#[derive(Debug, Default)]
pub struct RibbonOutcome {
    pub dismiss_id: Option<u64>,
    pub undo_entry_id: Option<u64>,
    pub open_sync: bool,
}

/// Timing & motion configuration for Status Ribbon rendering.
#[derive(Debug, Clone, Copy)]
pub struct RibbonTiming {
    pub now_ms: u64,
    pub elapsed_secs: f32,
    pub ticker_speed: f32,
    pub reduced_motion: bool,
}

/// Render the clickable right-side Status Ribbon ticker + last-50 history popup.
pub fn show_status_ribbon(
    ui: &mut egui::Ui,
    bus: &mut StatusBus,
    palette: &Palette,
    lang: ResolvedLang,
    timing: RibbonTiming,
    sync_queue: &mut SyncQueue,
    active_tab: &mut GuiTab,
) {
    let RibbonTiming {
        now_ms,
        elapsed_secs,
        ticker_speed,
        reduced_motion,
    } = timing;
    bus.prune_expired(now_ms);
    let current = bus.current(now_ms).cloned();
    let popup_id = ui.make_persistent_id("dalga_status_history_popup");
    let mut outcome = RibbonOutcome::default();

    let (icon, color, text) = match &current {
        Some(msg) => (
            priority_icon(msg),
            priority_color(msg, palette),
            msg.text.as_str(),
        ),
        None => (Icon::Bell, palette.text_muted, tr(lang, Key::RibbonEmpty)),
    };

    let width = 230.0_f32.min(ui.available_width().max(110.0));
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, 22.0), egui::Sense::click());
    let bg = if resp.hovered() {
        palette.bg_hover
    } else {
        palette.bg_elevated
    };
    ui.painter().rect(
        rect,
        4.0,
        bg,
        egui::Stroke::new(1.0_f32, palette.border_subtle),
        egui::StrokeKind::Inside,
    );

    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left() + 6.0, rect.center().y - 6.5),
        egui::vec2(13.0, 13.0),
    );
    icon.image(13.0, color).paint_at(ui, icon_rect);

    let text_clip = egui::Rect::from_min_max(
        egui::pos2(icon_rect.right() + 6.0, rect.top() + 2.0),
        egui::pos2(rect.right() - 6.0, rect.bottom() - 2.0),
    );
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let text_w = galley.size().x;
    let view_w = text_clip.width().max(1.0);
    let x_offset = scroll_offset(elapsed_secs, text_w, view_w, ticker_speed, reduced_motion);
    let y_pos = text_clip.center().y - galley.size().y * 0.5;
    ui.painter().with_clip_rect(text_clip).galley(
        egui::pos2(text_clip.left() + x_offset, y_pos),
        galley,
        color,
    );

    if text_w > view_w && !reduced_motion && current.is_some() {
        ui.ctx().request_repaint();
    }

    if resp
        .clone()
        .on_hover_text(tr(lang, Key::RibbonHistoryTitle))
        .clicked()
    {
        ui.memory_mut(|m| m.toggle_popup(popup_id));
    }

    if let Some(msg) = &current
        && msg.sticky
        && icon_button(ui, Icon::Close, 12.0, true, tr(lang, Key::RibbonDismiss)).clicked()
    {
        outcome.dismiss_id = Some(msg.id);
    }

    egui::popup_below_widget(
        ui,
        popup_id,
        &resp,
        egui::PopupCloseBehavior::CloseOnClickOutside,
        |ui| {
            ui.set_min_width(340.0);
            ui.set_max_height(320.0);
            ui.strong(tr(lang, Key::RibbonHistoryTitle));
            ui.separator();
            if let Some(msg) = &current {
                ui.horizontal(|ui| {
                    if let Some(eid) = msg.undo_entry_id
                        && ui.small_button(tr(lang, Key::RibbonUndo)).clicked()
                    {
                        outcome.undo_entry_id = Some(eid);
                    }
                    if msg.offer_sync_link
                        && *active_tab != GuiTab::Sync
                        && ui.small_button(tr(lang, Key::RibbonOpenSync)).clicked()
                    {
                        outcome.open_sync = true;
                    }
                    if ui.small_button(tr(lang, Key::RibbonDismiss)).clicked() {
                        outcome.dismiss_id = Some(msg.id);
                    }
                });
                ui.separator();
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                if bus.history().is_empty() {
                    ui.weak(tr(lang, Key::RibbonEmpty));
                } else {
                    for item in bus.history().iter().rev() {
                        let col = priority_color(item, palette);
                        ui.horizontal(|ui| {
                            paint_icon(ui, priority_icon(item), 12.0, col);
                            ui.colored_label(col, &item.text);
                        });
                    }
                }
            });
        },
    );

    if let Some(id) = outcome.dismiss_id {
        bus.dismiss(id);
    }
    if let Some(eid) = outcome.undo_entry_id {
        sync_queue.remove_entry(eid);
        bus.push_info(true, "Undid last staged action.", now_ms);
    }
    if outcome.open_sync {
        *active_tab = GuiTab::Sync;
    }
}

const fn priority_icon(msg: &StatusMessage) -> Icon {
    if msg.is_error || matches!(msg.priority, StatusPriority::ErrorWarning) {
        Icon::Warn
    } else if matches!(msg.priority, StatusPriority::ActionResult) {
        Icon::Check
    } else {
        Icon::Bell
    }
}

const fn priority_color(msg: &StatusMessage, pal: &Palette) -> Color32 {
    if msg.is_error {
        pal.status_err
    } else {
        match msg.priority {
            StatusPriority::ErrorWarning => pal.status_warn,
            StatusPriority::ActionResult => pal.status_ok,
            StatusPriority::ContextualHint | StatusPriority::Greeting => pal.text_muted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_offset_math_respects_reduced_motion_and_wraps() {
        assert_eq!(scroll_offset(2.0, 100.0, 200.0, 50.0, false), 0.0);
        assert_eq!(scroll_offset(2.0, 400.0, 200.0, 50.0, true), 0.0);

        let x0 = scroll_offset(0.0, 400.0, 200.0, 50.0, false);
        let x2 = scroll_offset(2.0, 400.0, 200.0, 50.0, false);
        assert!((x0 - 200.0).abs() < 1e-4);
        assert!((x2 - 100.0).abs() < 1e-4);
    }
}
