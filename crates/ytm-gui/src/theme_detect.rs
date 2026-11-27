use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;

use eframe::egui;

use crate::worker_msgs::WorkerEvent;

/// Parse XDG desktop portal `org.freedesktop.appearance color-scheme` output.
/// Returns `Some(true)` for dark (`1`), `Some(false)` for light (`2`), `None` for no preference (`0`) or unknown.
#[must_use]
pub fn parse_portal_color_scheme(raw: &str) -> Option<bool> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let val = trimmed
        .split(|c: char| !c.is_ascii_digit())
        .rfind(|s| !s.is_empty())?
        .parse::<u32>()
        .ok()?;
    match val {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Parse GNOME `gsettings get org.gnome.desktop.interface color-scheme` output.
#[must_use]
pub fn parse_gsettings_color_scheme(raw: &str) -> Option<bool> {
    let norm = raw.trim().trim_matches('\'').trim_matches('"');
    match norm {
        "prefer-dark" => Some(true),
        "prefer-light" => Some(false),
        _ => None,
    }
}

/// Parse Windows `reg query ... /v AppsUseLightTheme` stdout (`0x0` = dark, `0x1` = light).
#[must_use]
pub fn parse_windows_apps_use_light_theme(raw: &str) -> Option<bool> {
    for line in raw.lines() {
        if !line.contains("AppsUseLightTheme") {
            continue;
        }
        let token = line.split_whitespace().last()?;
        let val = if let Some(hex) = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            token.parse::<u32>().ok()?
        };
        return match val {
            0 => Some(true),
            1 => Some(false),
            _ => None,
        };
    }
    None
}

/// Probe the host OS dark/light preference (safe to call off the UI thread; defaults to dark).
#[must_use]
pub fn detect_os_dark_mode() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = Command::new("gdbus")
            .args([
                "call",
                "--session",
                "--dest",
                "org.freedesktop.portal.Desktop",
                "--object-path",
                "/org/freedesktop/portal/desktop",
                "--method",
                "org.freedesktop.portal.Settings.Read",
                "org.freedesktop.appearance",
                "color-scheme",
            ])
            .output()
            && out.status.success()
            && let Some(dark) = parse_portal_color_scheme(&String::from_utf8_lossy(&out.stdout))
        {
            return dark;
        }
        if let Ok(out) = Command::new("gsettings")
            .args(["get", "org.gnome.desktop.interface", "color-scheme"])
            .output()
            && out.status.success()
            && let Some(dark) = parse_gsettings_color_scheme(&String::from_utf8_lossy(&out.stdout))
        {
            return dark;
        }
        true
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(out) = Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "/v",
                "AppsUseLightTheme",
            ])
            .output()
            && out.status.success()
            && let Some(dark) =
                parse_windows_apps_use_light_theme(&String::from_utf8_lossy(&out.stdout))
        {
            return dark;
        }
        true
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        true
    }
}

/// Spawn a lightweight background watcher that probes OS dark/light preference every 3 seconds
/// and notifies the UI only when it changes (never blocks `eframe::App::update`).
pub(crate) fn spawn_theme_watcher(tx: Sender<WorkerEvent>, ctx: egui::Context) {
    tokio::spawn(async move {
        let mut last = tokio::task::spawn_blocking(detect_os_dark_mode)
            .await
            .unwrap_or(true);
        if tx.send(WorkerEvent::SystemThemeChanged(last)).is_err() {
            return;
        }
        ctx.request_repaint();
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        interval.tick().await;
        loop {
            interval.tick().await;
            let next = tokio::task::spawn_blocking(detect_os_dark_mode)
                .await
                .unwrap_or(last);
            if next != last {
                last = next;
                if tx.send(WorkerEvent::SystemThemeChanged(next)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_color_scheme_parses_0_1_2_and_dbus_formats() {
        assert_eq!(parse_portal_color_scheme("(<<uint32 0>>,)\n"), None);
        assert_eq!(parse_portal_color_scheme("(<<uint32 1>>,)\n"), Some(true));
        assert_eq!(parse_portal_color_scheme("(<<uint32 2>>,)\n"), Some(false));
        assert_eq!(parse_portal_color_scheme("variant uint32 1"), Some(true));
        assert_eq!(parse_portal_color_scheme("variant uint32 2"), Some(false));
        assert_eq!(parse_portal_color_scheme(""), None);
    }

    #[test]
    fn gsettings_color_scheme_parses_values() {
        assert_eq!(parse_gsettings_color_scheme("'prefer-dark'\n"), Some(true));
        assert_eq!(
            parse_gsettings_color_scheme("'prefer-light'\n"),
            Some(false)
        );
        assert_eq!(parse_gsettings_color_scheme("'default'\n"), None);
    }

    #[test]
    fn windows_registry_apps_use_light_theme_parses_0_and_1() {
        let dark_out = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\n    AppsUseLightTheme    REG_DWORD    0x0\n";
        let light_out = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\n    AppsUseLightTheme    REG_DWORD    0x1\n";
        assert_eq!(parse_windows_apps_use_light_theme(dark_out), Some(true));
        assert_eq!(parse_windows_apps_use_light_theme(light_out), Some(false));
        assert_eq!(parse_windows_apps_use_light_theme("unrelated"), None);
    }
}
