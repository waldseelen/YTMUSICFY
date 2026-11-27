use std::process::{Command, Stdio};

use crate::brand::APP_NAME;
use crate::state::GuiAppConfig;

#[must_use]
fn build_notify_args<'a>(title: &'a str, artist: &'a str) -> (&'a str, String) {
    let body = if artist.trim().is_empty() {
        "Now playing".to_string()
    } else {
        artist.trim().to_string()
    };
    (title.trim(), body)
}

/// Send a non-blocking OS notification on track change when explicitly enabled.
/// Performs zero OS/process calls when `config.notifications_enabled` is `false`.
pub fn notify_track_change(config: &GuiAppConfig, title: &str, artist: &str) {
    if !config.notifications_enabled || title.trim().is_empty() {
        return;
    }
    let (summary, body) = build_notify_args(title, artist);
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("notify-send")
            .args(["-a", APP_NAME, "--", summary, &body])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (APP_NAME, summary, body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_track_change_disabled_does_nothing() {
        let config = GuiAppConfig {
            notifications_enabled: false,
            ..Default::default()
        };
        notify_track_change(&config, "Test Track", "Test Artist");
    }

    #[test]
    fn build_notify_args_handles_blank_artist() {
        assert_eq!(
            build_notify_args("  Song  ", "  Artist  "),
            ("Song", "Artist".to_string())
        );
        assert_eq!(
            build_notify_args("Song", "   "),
            ("Song", "Now playing".to_string())
        );
    }
}
