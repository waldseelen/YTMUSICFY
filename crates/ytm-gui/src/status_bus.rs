use std::collections::VecDeque;

use crate::contracts::{StatusMessage, StatusPriority};

pub const HISTORY_CAP: usize = 50;
pub const ACTION_RESULT_TTL_MS: u64 = 5_000;
pub const CONTEXT_HINT_TTL_MS: u64 = 8_000;
pub const GREETING_TTL_MS: u64 = 10_000;

/// Priority-ordered status message bus with sticky errors and a 50-item history ring.
#[derive(Debug, Clone, Default)]
pub struct StatusBus {
    active: Vec<StatusMessage>,
    history: VecDeque<StatusMessage>,
    next_id: u64,
}

impl StatusBus {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn push_spec(&mut self, spec: StatusMessage) -> u64 {
        let id = spec.id;
        self.active.push(spec.clone());
        self.history.push_back(spec);
        while self.history.len() > HISTORY_CAP {
            self.history.pop_front();
        }
        id
    }

    pub fn push_info(&mut self, ok: bool, text: impl Into<String>, now_ms: u64) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let msg = if ok {
            StatusMessage {
                id,
                priority: StatusPriority::ActionResult,
                text: text.into(),
                is_error: false,
                sticky: false,
                ttl_ms: Some(ACTION_RESULT_TTL_MS),
                created_at_ms: now_ms,
                undo_entry_id: None,
                offer_sync_link: false,
            }
        } else {
            StatusMessage {
                id,
                priority: StatusPriority::ErrorWarning,
                text: text.into(),
                is_error: true,
                sticky: true,
                ttl_ms: None,
                created_at_ms: now_ms,
                undo_entry_id: None,
                offer_sync_link: false,
            }
        };
        self.push_spec(msg)
    }

    pub fn push_staged(
        &mut self,
        text: impl Into<String>,
        undo_entry_id: Option<u64>,
        now_ms: u64,
    ) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        let msg = StatusMessage {
            id: self.next_id,
            priority: StatusPriority::ActionResult,
            text: text.into(),
            is_error: false,
            sticky: false,
            ttl_ms: Some(ACTION_RESULT_TTL_MS),
            created_at_ms: now_ms,
            undo_entry_id,
            offer_sync_link: true,
        };
        self.push_spec(msg)
    }

    pub fn push_hint(&mut self, text: impl Into<String>, now_ms: u64) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        let msg = StatusMessage {
            id: self.next_id,
            priority: StatusPriority::ContextualHint,
            text: text.into(),
            is_error: false,
            sticky: false,
            ttl_ms: Some(CONTEXT_HINT_TTL_MS),
            created_at_ms: now_ms,
            undo_entry_id: None,
            offer_sync_link: false,
        };
        self.push_spec(msg)
    }

    pub fn push_greeting(&mut self, text: impl Into<String>, now_ms: u64) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        let msg = StatusMessage {
            id: self.next_id,
            priority: StatusPriority::Greeting,
            text: text.into(),
            is_error: false,
            sticky: false,
            ttl_ms: Some(GREETING_TTL_MS),
            created_at_ms: now_ms,
            undo_entry_id: None,
            offer_sync_link: false,
        };
        self.push_spec(msg)
    }

    pub fn prune_expired(&mut self, now_ms: u64) {
        self.active.retain(|m| !m.is_expired(now_ms));
    }

    /// Return the highest-priority active message (breaking ties by most recent).
    #[must_use]
    pub fn current(&self, now_ms: u64) -> Option<&StatusMessage> {
        self.active
            .iter()
            .filter(|m| !m.is_expired(now_ms))
            .max_by_key(|m| (m.priority, m.created_at_ms, m.id))
    }

    pub fn dismiss(&mut self, id: u64) {
        self.active.retain(|m| m.id != id);
    }

    pub fn clear_active(&mut self) {
        self.active.clear();
    }

    #[must_use]
    pub fn history(&self) -> &VecDeque<StatusMessage> {
        &self.history
    }

    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering_preempts_lower_messages() {
        let mut bus = StatusBus::new();
        bus.push_greeting("Good morning", 1000);
        assert_eq!(bus.current(1000).unwrap().text, "Good morning");

        bus.push_hint("Press Ctrl+K", 1100);
        assert_eq!(bus.current(1100).unwrap().text, "Press Ctrl+K");

        bus.push_info(true, "Saved", 1200);
        assert_eq!(bus.current(1200).unwrap().text, "Saved");

        let err_id = bus.push_info(false, "Network failed", 1300);
        assert_eq!(bus.current(1300).unwrap().text, "Network failed");

        // Error is sticky even far in the future until dismissed.
        assert_eq!(bus.current(999_999).unwrap().id, err_id);
        bus.dismiss(err_id);
        assert!(bus.current(999_999).is_none());
    }

    #[test]
    fn history_ring_evicts_oldest_beyond_50() {
        let mut bus = StatusBus::new();
        for i in 0..65 {
            bus.push_info(true, format!("msg-{i}"), i * 100);
        }
        assert_eq!(bus.history().len(), HISTORY_CAP);
        assert_eq!(bus.history().front().unwrap().text, "msg-15");
        assert_eq!(bus.history().back().unwrap().text, "msg-64");
    }
}
