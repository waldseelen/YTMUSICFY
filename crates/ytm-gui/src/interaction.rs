use std::collections::HashSet;

use eframe::egui;

use crate::icons::{Icon, icon_button};
use crate::state::GuiTab;
use crate::tab_bar::Workspace;

/// Decoded global keyboard & mouse-gesture actions for the current frame.
#[derive(Debug, Default, PartialEq)]
pub struct GlobalInput {
    pub switch_tab: Option<GuiTab>,
    pub focus_search: bool,
    pub trigger_sync: bool,
    pub toggle_sync_drawer: bool,
    pub toggle_palette: bool,
    pub undo_last: bool,
    pub redo_last: bool,
    pub toggle_play_pause: bool,
    pub toggle_mute: bool,
    pub toggle_mini_player: bool,
    pub open_ytm: bool,
    pub open_ytm_at_time: bool,
    pub zoom_delta: Option<f32>,
    pub reset_zoom: bool,
    pub nav_delta: i8,
}

/// Pure decoder for global keyboard & mouse shortcuts (unit-testable without a window).
#[must_use]
pub fn decode_global_input(i: &egui::InputState, typing: bool) -> GlobalInput {
    let mut out = GlobalInput::default();
    if i.modifiers.ctrl || i.modifiers.mac_cmd {
        let ws_keys = [
            egui::Key::Num1,
            egui::Key::Num2,
            egui::Key::Num3,
            egui::Key::Num4,
            egui::Key::Num5,
        ];
        for (idx, key) in ws_keys.iter().enumerate() {
            if i.key_pressed(*key)
                && let Some(&ws) = Workspace::ALL.get(idx)
            {
                out.switch_tab = Some(ws.default_tab());
            }
        }
        if i.key_pressed(egui::Key::K) {
            out.toggle_palette = true;
        }
        if i.modifiers.shift && i.key_pressed(egui::Key::S) {
            out.toggle_sync_drawer = true;
        }
        if i.key_pressed(egui::Key::F) {
            out.focus_search = true;
        }
        if i.key_pressed(egui::Key::M) {
            out.toggle_mini_player = true;
        }
        if i.key_pressed(egui::Key::O) {
            if i.modifiers.shift {
                out.open_ytm_at_time = true;
            } else {
                out.open_ytm = true;
            }
        }
        if i.key_pressed(egui::Key::Enter) {
            out.trigger_sync = true;
        }
        if i.key_pressed(egui::Key::Z) && !typing {
            if i.modifiers.shift {
                out.redo_last = true;
            } else {
                out.undo_last = true;
            }
        }
        if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
            out.zoom_delta = Some(0.1);
        } else if i.key_pressed(egui::Key::Minus) {
            out.zoom_delta = Some(-0.1);
        } else if i.key_pressed(egui::Key::Num0) {
            out.reset_zoom = true;
        }
    } else if !typing {
        if i.key_pressed(egui::Key::Space) {
            out.toggle_play_pause = true;
        }
        if i.key_pressed(egui::Key::M) {
            out.toggle_mute = true;
        }
        if !i.modifiers.alt && i.key_pressed(egui::Key::Slash) {
            out.focus_search = true;
        }
    }
    if i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft) {
        out.nav_delta = -1;
    } else if i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight) {
        out.nav_delta = 1;
    }
    if i.pointer.button_pressed(egui::PointerButton::Extra1) {
        out.nav_delta = -1;
    } else if i.pointer.button_pressed(egui::PointerButton::Extra2) {
        out.nav_delta = 1;
    }
    out
}

/// Polls global shortcuts (`Ctrl+1..5` Workspaces, `Ctrl+K`, `Ctrl+Shift+S`, `Ctrl+F`, `Ctrl+O`, `Ctrl+Shift+O`, `Ctrl+Enter`, `Ctrl+Z`, `Ctrl+Shift+Z`, `Space`, `M`, `Ctrl+/-/0`, `Alt+Left/Right`, mouse Extra1/Extra2).
#[must_use]
pub fn poll_global_input(ctx: &egui::Context) -> GlobalInput {
    let typing = ctx.wants_keyboard_input();
    ctx.input(|i| decode_global_input(i, typing))
}

/// Formats status labels with color-blind friendly text prefixes.
#[must_use]
pub fn status_badge(ok: bool, text: &str) -> String {
    let prefix = if ok { "OK:" } else { "ERR:" };
    format!("{prefix} {text}")
}

/// Output from `search_box_with_clear`.
#[derive(Debug, Default)]
pub struct SearchBoxOutput {
    pub submitted: bool,
    pub cleared: bool,
}

/// Reusable single-line search/filter input with clear icon button, `Enter` submit, and optional auto-focus.
pub fn search_box_with_clear(
    ui: &mut egui::Ui,
    id_salt: &str,
    text: &mut String,
    hint: &str,
    width: f32,
    focus_now: bool,
) -> SearchBoxOutput {
    let mut out = SearchBoxOutput::default();
    ui.horizontal(|ui| {
        let resp = ui.add(
            egui::TextEdit::singleline(text)
                .id(egui::Id::new(id_salt))
                .hint_text(hint)
                .desired_width(width),
        );
        if focus_now {
            resp.request_focus();
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            out.submitted = true;
        }
        if !text.is_empty() && icon_button(ui, Icon::Close, 14.0, true, "Clear").clicked() {
            text.clear();
            out.cleared = true;
            resp.request_focus();
        }
    });
    out
}

/// Auto-scrolls a scrollable viewport vertically when dragging near top/bottom edges.
pub fn auto_scroll_on_drag(ui: &mut egui::Ui, clip_rect: egui::Rect) {
    let Some(pos) = ui.ctx().pointer_latest_pos() else {
        return;
    };
    if !egui::DragAndDrop::has_any_payload(ui.ctx()) || !clip_rect.contains(pos) {
        return;
    }
    let edge = 36.0_f32;
    let dy = if pos.y - clip_rect.top() < edge {
        ((edge - (pos.y - clip_rect.top())) / edge) * 14.0
    } else if clip_rect.bottom() - pos.y < edge {
        -((edge - (clip_rect.bottom() - pos.y)) / edge) * 14.0
    } else {
        0.0
    };
    if dy.abs() > 0.1 {
        ui.scroll_with_delta(egui::vec2(0.0, dy));
        ui.ctx().request_repaint();
    }
}

/// Paints a floating drag badge near the cursor showing item summary.
pub fn paint_drag_ghost(ctx: &egui::Context, label: &str) {
    let Some(pos) = ctx.pointer_latest_pos() else {
        return;
    };
    egui::Area::new(egui::Id::new("ytm_drag_ghost"))
        .interactable(false)
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(14.0, 14.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.strong(label);
            });
        });
}

/// Computes `Shift+Click` range selection over visible track positions.
pub fn apply_range_select(
    visible_positions: &[usize],
    anchor: usize,
    target: usize,
    selected: &mut HashSet<usize>,
) {
    let Some(i1) = visible_positions.iter().position(|&p| p == anchor) else {
        selected.insert(target);
        return;
    };
    let Some(i2) = visible_positions.iter().position(|&p| p == target) else {
        selected.insert(target);
        return;
    };
    let (lo, hi) = if i1 <= i2 { (i1, i2) } else { (i2, i1) };
    for &pos in &visible_positions[lo..=hi] {
        selected.insert(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_select_marks_forward_and_backward_spans() {
        let visible = vec![10, 20, 30, 40, 50];
        let mut sel = HashSet::new();
        apply_range_select(&visible, 40, 20, &mut sel);
        assert_eq!(sel, HashSet::from([20, 30, 40]));
    }

    #[test]
    fn global_input_decodes_ctrl_o_and_ctrl_shift_o_shortcuts() {
        let raw_o = egui::RawInput {
            modifiers: egui::Modifiers::CTRL,
            events: vec![egui::Event::Key {
                key: egui::Key::O,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        let mut state_o = egui::InputState::default();
        state_o = state_o.begin_pass(raw_o, false, 1.0, &Default::default());
        let decoded_o = decode_global_input(&state_o, false);
        assert!(decoded_o.open_ytm);
        assert!(!decoded_o.open_ytm_at_time);

        let raw_shift_o = egui::RawInput {
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            events: vec![egui::Event::Key {
                key: egui::Key::O,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::SHIFT,
            }],
            ..Default::default()
        };
        let mut state_so = egui::InputState::default();
        state_so = state_so.begin_pass(raw_shift_o, false, 1.0, &Default::default());
        let decoded_so = decode_global_input(&state_so, false);
        assert!(decoded_so.open_ytm_at_time);
        assert!(!decoded_so.open_ytm);
    }

    #[test]
    fn global_input_decodes_slash_search_and_ctrl_m_mini_player() {
        let raw_slash = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Slash,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let state_slash = egui::InputState::default().begin_pass(
            raw_slash.clone(),
            false,
            1.0,
            &Default::default(),
        );
        assert!(decode_global_input(&state_slash, false).focus_search);
        assert!(!decode_global_input(&state_slash, true).focus_search);

        let raw_ctrl_m = egui::RawInput {
            modifiers: egui::Modifiers::CTRL,
            events: vec![egui::Event::Key {
                key: egui::Key::M,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            }],
            ..Default::default()
        };
        let state_m =
            egui::InputState::default().begin_pass(raw_ctrl_m, false, 1.0, &Default::default());
        assert!(decode_global_input(&state_m, false).toggle_mini_player);
    }
}
