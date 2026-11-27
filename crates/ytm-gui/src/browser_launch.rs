use std::process::Command;

pub const REFRESH_WAIT_MS: u64 = 7_000;

/// Build a YouTube Music `watch?v=` URL (or base URL when no track is active).
#[must_use]
pub fn build_watch_url(video_id: Option<&str>) -> String {
    match video_id.map(str::trim).filter(|s| !s.is_empty()) {
        Some(vid) => format!("https://music.youtube.com/watch?v={vid}"),
        None => "https://music.youtube.com/".to_string(),
    }
}

/// Construct `(program, args)` to open `url` cross-platform without spawning a process.
#[must_use]
pub fn build_open_url_command(url: &str, preferred_browser: Option<&str>) -> (String, Vec<String>) {
    if let Some(browser) = preferred_browser.map(str::trim).filter(|s| !s.is_empty()) {
        return (browser.to_string(), vec![url.to_string()]);
    }
    #[cfg(target_os = "windows")]
    {
        (
            "cmd".to_string(),
            vec![
                "/C".to_string(),
                "start".to_string(),
                String::new(),
                url.to_string(),
            ],
        )
    }
    #[cfg(target_os = "macos")]
    {
        ("open".to_string(), vec![url.to_string()])
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ("xdg-open".to_string(), vec![url.to_string()])
    }
}

/// Launch `url` in the system default or configured browser without blocking.
pub fn open_url_in_browser(url: &str, preferred_browser: Option<&str>) -> std::io::Result<()> {
    let (prog, args) = build_open_url_command(url, preferred_browser);
    Command::new(prog).args(args).spawn().map(|_| ())
}

/// Explicit state machine for guided one-click Session Refresh (S1).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SessionRefreshState {
    #[default]
    Idle,
    Opening,
    WaitingToImport {
        started_ms: u64,
        wait_ms: u64,
    },
    Importing,
    Done,
    Failed(String),
}

impl SessionRefreshState {
    pub fn begin(&mut self) {
        *self = Self::Opening;
    }

    pub fn mark_waiting(&mut self, started_ms: u64) {
        *self = Self::WaitingToImport {
            started_ms,
            wait_ms: REFRESH_WAIT_MS,
        };
    }

    #[must_use]
    pub fn should_import(&self, now_ms: u64) -> bool {
        match *self {
            Self::WaitingToImport {
                started_ms,
                wait_ms,
            } => now_ms.saturating_sub(started_ms) >= wait_ms,
            _ => false,
        }
    }

    pub fn mark_importing(&mut self) {
        *self = Self::Importing;
    }

    pub fn finish_ok(&mut self) {
        *self = Self::Done;
    }

    pub fn finish_err(&mut self, err: impl Into<String>) {
        *self = Self::Failed(err.into());
    }

    #[must_use]
    pub const fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Opening | Self::WaitingToImport { .. } | Self::Importing
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_watch_url_handles_some_and_none() {
        assert_eq!(build_watch_url(None), "https://music.youtube.com/");
        assert_eq!(build_watch_url(Some("   ")), "https://music.youtube.com/");
        assert_eq!(
            build_watch_url(Some("dQw4w9WgXcQ")),
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ"
        );
    }

    #[test]
    fn build_open_url_command_uses_override_or_os_launcher() {
        let (prog, args) = build_open_url_command("https://music.youtube.com/", Some("brave"));
        assert_eq!(prog, "brave");
        assert_eq!(args, vec!["https://music.youtube.com/"]);

        let (def_prog, def_args) = build_open_url_command("https://music.youtube.com/", None);
        if cfg!(target_os = "windows") {
            assert_eq!(def_prog, "cmd");
            assert!(def_args.contains(&"start".to_string()));
        } else if cfg!(target_os = "macos") {
            assert_eq!(def_prog, "open");
        } else {
            assert_eq!(def_prog, "xdg-open");
            assert_eq!(def_args, vec!["https://music.youtube.com/"]);
        }
    }

    #[test]
    fn session_refresh_state_machine_transitions() {
        let mut s = SessionRefreshState::default();
        assert_eq!(s, SessionRefreshState::Idle);
        assert!(!s.is_busy());

        s.begin();
        assert!(s.is_busy());

        s.mark_waiting(10_000);
        assert!(s.is_busy());
        assert!(!s.should_import(16_999));
        assert!(s.should_import(17_000));

        s.mark_importing();
        assert!(s.is_busy());

        s.finish_ok();
        assert_eq!(s, SessionRefreshState::Done);
        assert!(!s.is_busy());

        s.finish_err("cookie missing");
        assert_eq!(s, SessionRefreshState::Failed("cookie missing".to_string()));
    }
}
