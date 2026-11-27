use crate::state::GuiAppConfig;

/// Send a desktop notification for track change.
/// Does nothing if notifications are disabled in config.
/// Actual platform-specific implementation is a stub for now.
pub fn notify_track_change(_config: &GuiAppConfig, _title: &str, _artist: &str) {
    // Stub: Desktop notifications implementation pending
    // On Linux: use notify-rust
    // On Windows: use windows-toast or similar
    // For now, this is a no-op when disabled, and a stub when enabled.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::GuiAppConfig;

    #[test]
    fn notify_track_change_disabled_does_nothing() {
        let config = GuiAppConfig {
            notifications_enabled: false,
            ..Default::default()
        };
        // Should not panic or do anything
        notify_track_change(&config, "Test Track", "Test Artist");
    }
}
