use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use ytm_core::Track;

use crate::player::{PlayerSnapshot, PlayerState};

/// Normalized OS media key events forwarded from MPRIS2 (Linux) or SMTC (Windows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKeyEvent {
    TogglePause,
    Play,
    Pause,
    Next,
    Previous,
    Stop,
}

/// Cross-platform OS media controls bridge (`souvlaki` MPRIS2 / SMTC) with headless safety.
pub struct OsMediaBridge {
    controls: Option<MediaControls>,
    rx: Receiver<MediaKeyEvent>,
    last_video_id: Option<String>,
    last_playing: Option<bool>,
}

impl OsMediaBridge {
    /// Initialize OS media controls without a window handle (safe no-op on headless/Windows tests).
    #[must_use]
    pub fn new() -> Self {
        Self::with_hwnd(None)
    }

    /// Initialize OS media controls from `eframe::CreationContext` (extracts Win32 `HWND` on Windows).
    #[must_use]
    pub fn from_creation_context(cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(windows)]
        {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            let hwnd = cc.window_handle().ok().and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(win32) => Some(win32.hwnd.get() as *mut std::ffi::c_void),
                _ => None,
            });
            Self::with_hwnd(hwnd)
        }
        #[cfg(not(windows))]
        {
            let _ = cc;
            Self::with_hwnd(None)
        }
    }

    /// Initialize OS media controls with an optional Win32 `HWND` (`SMTC` on Windows, `MPRIS2` on Linux).
    #[must_use]
    pub fn with_hwnd(hwnd: Option<*mut std::ffi::c_void>) -> Self {
        let (tx, rx) = mpsc::channel::<MediaKeyEvent>();
        #[cfg(windows)]
        let can_init = hwnd.is_some();
        #[cfg(not(windows))]
        let can_init = true;

        let controls = if can_init {
            let config = PlatformConfig {
                dbus_name: "ytm_architect",
                display_name: "ytm-architect",
                hwnd,
            };
            MediaControls::new(config).ok().and_then(|mut c| {
                let attach_res = c.attach(move |ev: MediaControlEvent| {
                    let mapped = match ev {
                        MediaControlEvent::Toggle => Some(MediaKeyEvent::TogglePause),
                        MediaControlEvent::Play => Some(MediaKeyEvent::Play),
                        MediaControlEvent::Pause => Some(MediaKeyEvent::Pause),
                        MediaControlEvent::Next => Some(MediaKeyEvent::Next),
                        MediaControlEvent::Previous => Some(MediaKeyEvent::Previous),
                        MediaControlEvent::Stop => Some(MediaKeyEvent::Stop),
                        _ => None,
                    };
                    if let Some(m) = mapped {
                        let _ = tx.send(m);
                    }
                });
                attach_res.ok().map(|()| c)
            })
        } else {
            None
        };

        Self {
            controls,
            rx,
            last_video_id: None,
            last_playing: None,
        }
    }

    /// Drain pending OS media key events without blocking.
    pub fn poll_events(&self) -> Vec<MediaKeyEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Synchronize current track metadata and playback state to MPRIS2 / SMTC when changed.
    pub fn sync_state(&mut self, current: Option<&Track>, snap: &PlayerSnapshot) {
        let Some(controls) = self.controls.as_mut() else {
            return;
        };

        let cur_vid = current.map(|t| t.video_id.as_str());
        if self.last_video_id.as_deref() != cur_vid {
            self.last_video_id = cur_vid.map(str::to_string);
            if let Some(t) = current {
                let _ = controls.set_metadata(MediaMetadata {
                    title: Some(&t.title),
                    artist: t.primary_artist_name(),
                    album: t.album.as_ref().map(|a| a.name.as_str()),
                    cover_url: t.thumbnail_url.as_deref(),
                    duration: t.duration_sec.map(|s| Duration::from_secs(u64::from(s))),
                });
            }
        }

        let is_playing = matches!(snap.state, PlayerState::Playing { .. });
        let is_active = snap.state.is_active();
        let next_flag = is_active.then_some(is_playing);
        if self.last_playing != next_flag {
            self.last_playing = next_flag;
            let pb = match next_flag {
                Some(true) => MediaPlayback::Playing { progress: None },
                Some(false) => MediaPlayback::Paused { progress: None },
                None => MediaPlayback::Stopped,
            };
            let _ = controls.set_playback(pb);
        }
    }
}

impl Default for OsMediaBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_media_bridge_initializes_gracefully_without_panicking() {
        let mut bridge = OsMediaBridge::new();
        bridge.sync_state(None, &PlayerSnapshot::default());
        assert!(bridge.poll_events().is_empty());
    }
}
