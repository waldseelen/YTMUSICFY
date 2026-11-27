use serde::{Deserialize, Serialize};

/// Application surface theme mode (`System` follows OS light/dark preference).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

impl ThemeMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];
}

/// Selectable static accent color palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AccentPalette {
    #[default]
    Emerald,
    Gold,
    YtmPink,
}

impl AccentPalette {
    pub const ALL: [Self; 3] = [Self::Emerald, Self::Gold, Self::YtmPink];
}

/// UI localization setting (`System` picks OS locale, defaulting to English).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum UiLang {
    #[default]
    System,
    En,
    Tr,
}

impl UiLang {
    pub const ALL: [Self; 3] = [Self::System, Self::En, Self::Tr];
}

/// Priority levels for the Status Ribbon message bus (higher value preempts lower).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum StatusPriority {
    Greeting = 0,
    ContextualHint = 1,
    ActionResult = 2,
    ErrorWarning = 3,
}

/// Display density mode for the Mini Player secondary viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MiniPlayerMode {
    #[default]
    Compact,
    Expanded,
}

/// Persisted Mini Player position, size, and mode per monitor key.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MiniPlayerGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub mode: MiniPlayerMode,
}

/// Pure window close-request reducer outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    None,
    HideToTray,
    Quit,
}

/// User-facing window close action preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WindowCloseAction {
    /// Exit the application completely.
    Quit,
    /// Hide window to system tray (requires tray host on Wayland).
    #[default]
    HideToTray,
    /// Minimize to taskbar / dock (always works).
    Minimize,
    /// Hide window but keep running in background (no tray icon).
    HideWindow,
}

/// Structured message carried by the Status Ribbon bus and history ring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusMessage {
    pub id: u64,
    pub priority: StatusPriority,
    pub text: String,
    pub is_error: bool,
    pub sticky: bool,
    pub ttl_ms: Option<u64>,
    pub created_at_ms: u64,
    #[serde(default)]
    pub undo_entry_id: Option<u64>,
    #[serde(default)]
    pub offer_sync_link: bool,
}

impl StatusMessage {
    /// Returns true when the message has a finite TTL that has elapsed at `now_ms`.
    #[must_use]
    pub fn is_expired(&self, now_ms: u64) -> bool {
        if self.sticky {
            return false;
        }
        self.ttl_ms
            .is_some_and(|ttl| now_ms.saturating_sub(self.created_at_ms) >= ttl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_priority_orders_error_above_result_hint_greeting() {
        assert!(StatusPriority::ErrorWarning > StatusPriority::ActionResult);
        assert!(StatusPriority::ActionResult > StatusPriority::ContextualHint);
        assert!(StatusPriority::ContextualHint > StatusPriority::Greeting);
    }

    #[test]
    fn status_message_serde_roundtrip_and_expiry() {
        let msg = StatusMessage {
            id: 42,
            priority: StatusPriority::ActionResult,
            text: "Staged 3 tracks".to_string(),
            is_error: false,
            sticky: false,
            ttl_ms: Some(5000),
            created_at_ms: 10_000,
            undo_entry_id: Some(7),
            offer_sync_link: true,
        };
        let raw = serde_json::to_string(&msg).expect("serialize");
        let back: StatusMessage = serde_json::from_str(&raw).expect("deserialize");
        assert_eq!(back, msg);
        assert!(!back.is_expired(14_999));
        assert!(back.is_expired(15_000));
    }
}
