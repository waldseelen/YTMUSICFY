use std::time::{Duration, Instant};

use eframe::egui;

use crate::interaction::{poll_global_input, status_badge};
use crate::state::{ConfirmedMutation, GuiTab, PendingMutation};
use crate::sync_queue::SyncQueue;

const TOAST_TTL: Duration = Duration::from_secs(6);

/// User notification toast supporting auto-expiry, one-click undo, and Sync navigation.
#[derive(Debug, Clone)]
pub struct ToastState {
    pub ok: bool,
    pub message: String,
    pub expires_at: Instant,
    pub undo_entry_id: Option<u64>,
    pub offer_sync_link: bool,
}

impl ToastState {
    #[must_use]
    pub fn info(ok: bool, message: impl Into<String>) -> Self {
        Self::info_at(ok, message, Instant::now())
    }

    #[must_use]
    pub fn info_at(ok: bool, message: impl Into<String>, now: Instant) -> Self {
        Self {
            ok,
            message: message.into(),
            expires_at: now + TOAST_TTL,
            undo_entry_id: None,
            offer_sync_link: false,
        }
    }

    #[must_use]
    pub fn staged(message: impl Into<String>, undo_entry_id: Option<u64>) -> Self {
        Self::staged_at(message, undo_entry_id, Instant::now())
    }

    #[must_use]
    pub fn staged_at(message: impl Into<String>, undo_entry_id: Option<u64>, now: Instant) -> Self {
        Self {
            ok: true,
            message: message.into(),
            expires_at: now + TOAST_TTL,
            undo_entry_id,
            offer_sync_link: true,
        }
    }

    #[must_use]
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.expires_at
    }
}

/// Snapshot of current navigation target for Back/Forward mouse and keyboard gestures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavEntry {
    pub tab: GuiTab,
    pub playlist_id: Option<String>,
}

/// Bounded browser-style Back/Forward navigation history.
#[derive(Debug, Clone)]
pub struct NavHistory {
    current: NavEntry,
    back_stack: Vec<NavEntry>,
    forward_stack: Vec<NavEntry>,
    max_len: usize,
}

impl NavHistory {
    #[must_use]
    pub fn new(tab: GuiTab, playlist_id: Option<String>) -> Self {
        Self {
            current: NavEntry { tab, playlist_id },
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            max_len: 64,
        }
    }

    pub fn record(&mut self, tab: GuiTab, playlist_id: Option<String>) {
        let next = NavEntry { tab, playlist_id };
        if next == self.current {
            return;
        }
        let prev = std::mem::replace(&mut self.current, next);
        self.back_stack.push(prev);
        if self.back_stack.len() > self.max_len {
            self.back_stack.remove(0);
        }
        self.forward_stack.clear();
    }

    pub fn go_back(&mut self) -> Option<NavEntry> {
        let prev = self.back_stack.pop()?;
        let cur = std::mem::replace(&mut self.current, prev.clone());
        self.forward_stack.push(cur);
        Some(prev)
    }

    pub fn go_forward(&mut self) -> Option<NavEntry> {
        let next = self.forward_stack.pop()?;
        let cur = std::mem::replace(&mut self.current, next.clone());
        self.back_stack.push(cur);
        Some(next)
    }

    #[must_use]
    pub fn can_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    #[must_use]
    pub fn can_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }
}

/// Bottom status bar displaying progress, Cancel job button, expiring notifications, Undo, and Sync shortcut.
pub fn show_status_bar(
    ui: &mut egui::Ui,
    batch_progress: Option<&(usize, usize, String)>,
    active_job_id: Option<crate::state::JobId>,
    toast: &mut Option<ToastState>,
    sync_queue: &mut SyncQueue,
    active_tab: &mut GuiTab,
    cmd_tx: &std::sync::mpsc::Sender<crate::state::WorkerCmd>,
) {
    let now = Instant::now();
    if toast.as_ref().is_some_and(|t| t.is_expired(now)) {
        *toast = None;
    }
    ui.horizontal(|ui| {
        ui.label("Ctrl+1..5 Workspaces | Ctrl+K Palette | Ctrl+F Search | Ctrl+Z Undo | Ctrl+Enter Sync");
        if let Some((done, total, msg)) = batch_progress {
            ui.separator();
            let frac = (*done as f32 / (*total).max(1) as f32).clamp(0.0, 1.0);
            ui.add(egui::ProgressBar::new(frac).text(msg).desired_width(200.0));
            if let Some(jid) = active_job_id
                && ui
                    .small_button("Cancel")
                    .on_hover_text("Cancel running background job")
                    .clicked()
            {
                let _ = cmd_tx.send(crate::state::WorkerCmd::CancelJob(jid));
            }
        }
        let mut clear_toast = false;
        let mut undo_id = None;
        let mut jump_sync = false;
        if let Some(t) = toast {
            ui.separator();
            let color = if t.ok {
                egui::Color32::from_rgb(90, 200, 120)
            } else {
                egui::Color32::from_rgb(235, 95, 95)
            };
            ui.colored_label(color, status_badge(t.ok, &t.message));
            if let Some(eid) = t.undo_entry_id
                && ui.small_button("Undo").clicked()
            {
                undo_id = Some(eid);
                clear_toast = true;
            }
            if t.offer_sync_link
                && *active_tab != GuiTab::Sync
                && ui.small_button("Sync →").clicked()
            {
                jump_sync = true;
            }
            if ui.small_button("✕").on_hover_text("Dismiss").clicked() {
                clear_toast = true;
            }
        }
        if let Some(eid) = undo_id {
            sync_queue.remove_entry(eid);
            *toast = Some(ToastState::info(true, "Undid last staged action."));
            clear_toast = false;
        }
        if jump_sync {
            *active_tab = GuiTab::Sync;
        }
        if clear_toast {
            *toast = None;
        }
    });
}

/// Outcome flags returned by `apply_global_input` for `YtmGuiApp` orchestration.
#[derive(Debug, Default, Clone, Copy)]
pub struct GlobalInputOutcome {
    pub navigated_via_history: bool,
    pub toggle_palette: bool,
    pub toggle_sync_drawer: bool,
    pub toggle_play_pause: bool,
    pub toggle_mute: bool,
}

/// Applies global keyboard shortcuts, zoom, Sync shortcut, Undo/Redo, and Back/Forward navigation.
#[allow(clippy::too_many_arguments)]
pub fn apply_global_input(
    ctx: &egui::Context,
    base_ppp: f32,
    ui_scale: &mut f32,
    active_tab: &mut GuiTab,
    selected_playlist_id: &mut Option<String>,
    focus_search_once: &mut bool,
    sync_queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
    pending_modal: &mut Option<PendingMutation>,
    nav_history: &mut NavHistory,
) -> GlobalInputOutcome {
    let g = poll_global_input(ctx);
    let mut out = GlobalInputOutcome {
        navigated_via_history: false,
        toggle_palette: g.toggle_palette,
        toggle_sync_drawer: g.toggle_sync_drawer,
        toggle_play_pause: g.toggle_play_pause,
        toggle_mute: g.toggle_mute,
    };
    if g.undo_last
        && sync_queue.undo_last().is_some()
    {
        *toast = Some(ToastState::info(true, "Undid last staged action (Ctrl+Z)."));
    } else if g.redo_last
        && sync_queue.redo_last().is_some()
    {
        *toast = Some(ToastState::info(
            true,
            "Redid staged action (Ctrl+Shift+Z).",
        ));
    }
    if let Some(delta) = g.zoom_delta {
        *ui_scale = (*ui_scale + delta).clamp(0.8, 1.6);
    } else if g.reset_zoom {
        *ui_scale = 1.0;
    }
    let target_ppp = (base_ppp * *ui_scale).clamp(0.75, 3.0);
    if (ctx.pixels_per_point() - target_ppp).abs() > 0.01 {
        ctx.set_pixels_per_point(target_ppp);
    }
    if g.focus_search {
        *focus_search_once = true;
    }
    if let Some(tab) = g.switch_tab {
        *active_tab = tab;
    }
    if g.trigger_sync && !sync_queue.is_empty() && pending_modal.is_none() {
        let entries = sync_queue.selected_entries();
        if !entries.is_empty() {
            let affected = sync_queue.affected_playlist_ids();
            *pending_modal = Some(PendingMutation {
                title: "Confirm Sync to YouTube Music".to_string(),
                summary: format!(
                    "Save pre-sync SQLite snapshots for {} playlist(s) and execute {} queued action(s)?",
                    affected.len(),
                    entries.len()
                ),
                action: ConfirmedMutation::ExecuteSync { entries },
            });
        }
    }
    if g.nav_delta < 0
        && let Some(entry) = nav_history.go_back()
    {
        *active_tab = entry.tab;
        *selected_playlist_id = entry.playlist_id;
        out.navigated_via_history = true;
    } else if g.nav_delta > 0
        && let Some(entry) = nav_history.go_forward()
    {
        *active_tab = entry.tab;
        *selected_playlist_id = entry.playlist_id;
        out.navigated_via_history = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_history_records_back_and_forward_and_resets_forward_on_branch() {
        let mut nav = NavHistory::new(GuiTab::Inspector, None);
        nav.record(GuiTab::Inspector, Some("PL1".to_string()));
        nav.record(GuiTab::Dedupe, Some("PL1".to_string()));
        assert!(nav.can_back());
        assert!(!nav.can_forward());

        let b1 = nav.go_back().expect("back 1");
        assert_eq!(b1.tab, GuiTab::Inspector);
        assert_eq!(b1.playlist_id.as_deref(), Some("PL1"));
        assert!(nav.can_forward());

        let f1 = nav.go_forward().expect("forward 1");
        assert_eq!(f1.tab, GuiTab::Dedupe);

        let _ = nav.go_back();
        nav.record(GuiTab::Sync, Some("PL1".to_string()));
        assert!(!nav.can_forward());
    }

    #[test]
    fn toast_state_expires_after_ttl() {
        let t0 = Instant::now();
        let toast = ToastState::staged_at("Added", Some(7), t0);
        assert!(!toast.is_expired(t0 + Duration::from_secs(3)));
        assert!(toast.is_expired(t0 + Duration::from_secs(6)));
        assert_eq!(toast.undo_entry_id, Some(7));
    }
}
