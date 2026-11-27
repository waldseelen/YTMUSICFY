use eframe::egui;

use crate::i18n::{Key, ResolvedLang, tr};

const TRAY_SVG: &[u8] = include_bytes!("../../../assets/brand/dalga-tray-16.svg");
const TRAY_PNG: &[u8] = include_bytes!("../../../assets/brand/dalga-tray-16.png");

/// Actions emitted by the system tray icon or context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    ToggleWindow,
    PlayPause,
    Next,
    Previous,
    ToggleMiniPlayer,
    Quit,
}

/// Live UI/playback state synchronized to the system tray menu and icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraySnapshot {
    pub window_visible: bool,
    pub is_playing: bool,
    pub mini_player_open: bool,
    pub dark_theme: bool,
    pub track_label: Option<String>,
    pub lang: ResolvedLang,
}

impl Default for TraySnapshot {
    fn default() -> Self {
        Self {
            window_visible: true,
            is_playing: false,
            mini_player_open: false,
            dark_theme: true,
            track_label: None,
            lang: ResolvedLang::En,
        }
    }
}

/// Platform-agnostic system tray backend contract.
pub trait TrayBackend {
    fn is_active(&self) -> bool;
    fn poll_events(&self) -> Vec<TrayCommand>;
    fn sync_state(&mut self, snap: &TraySnapshot);
}

/// Fallback backend when tray is disabled or no OS tray host is available.
#[derive(Debug, Default)]
pub struct NoopTrayBackend;

impl TrayBackend for NoopTrayBackend {
    fn is_active(&self) -> bool {
        false
    }

    fn poll_events(&self) -> Vec<TrayCommand> {
        Vec::new()
    }

    fn sync_state(&mut self, _snap: &TraySnapshot) {}
}

/// Localized labels used by platform tray menus.
pub(crate) struct TrayMenuLabels {
    pub show_hide: &'static str,
    pub play_pause: &'static str,
    pub next: &'static str,
    pub prev: &'static str,
    pub mini_player: &'static str,
    pub quit: &'static str,
}

#[must_use]
pub(crate) fn menu_labels(snap: &TraySnapshot) -> TrayMenuLabels {
    let lang = snap.lang;
    TrayMenuLabels {
        show_hide: if snap.window_visible {
            tr(lang, Key::TrayHide)
        } else {
            tr(lang, Key::TrayShow)
        },
        play_pause: if snap.is_playing {
            tr(lang, Key::PlayerPause)
        } else {
            tr(lang, Key::PlayerPlay)
        },
        next: tr(lang, Key::PlayerNext),
        prev: tr(lang, Key::PlayerPrev),
        mini_player: tr(lang, Key::TrayMiniPlayer),
        quit: tr(lang, Key::TrayQuit),
    }
}

/// Render a crisp theme-aware RGBA tray icon from embedded Dalga brand assets.
#[must_use]
pub fn build_tray_rgba(dark_theme: bool) -> (Vec<u8>, u32, u32) {
    let (mut rgba, w, h) = if let Ok(img) =
        egui_extras::image::load_svg_bytes_with_size(TRAY_SVG, Some(egui::SizeHint::Size(32, 32)))
    {
        let mut raw = Vec::with_capacity(img.pixels.len() * 4);
        for c in &img.pixels {
            raw.extend_from_slice(&[c.r(), c.g(), c.b(), c.a()]);
        }
        (raw, img.size[0] as u32, img.size[1] as u32)
    } else if let Ok(dyn_img) = image::load_from_memory(TRAY_PNG) {
        let buf = dyn_img.into_rgba8();
        let (w, h) = buf.dimensions();
        (buf.into_raw(), w, h)
    } else {
        return (vec![255; 16 * 16 * 4], 16, 16);
    };

    let [tr, tg, tb] = if dark_theme {
        [244_u8, 246_u8, 250_u8]
    } else {
        [22_u8, 24_u8, 30_u8]
    };
    for px in rgba.chunks_exact_mut(4) {
        if px[3] > 0 {
            px[0] = tr;
            px[1] = tg;
            px[2] = tb;
        }
    }
    (rgba, w, h)
}

/// Convert packed RGBA bytes into network-byte-order ARGB32 for `ksni::Icon`.
#[cfg(any(target_os = "linux", test))]
#[must_use]
pub(crate) fn rgba_to_argb32(rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        out.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    out
}

/// Manages active OS tray backend lifecycle and throttles redundant state syncs.
pub struct TrayController {
    enabled: bool,
    backend: Box<dyn TrayBackend>,
    last_synced: Option<TraySnapshot>,
}

impl TrayController {
    #[must_use]
    pub fn new(enabled: bool, ctx: Option<egui::Context>) -> Self {
        let backend = create_backend(enabled, ctx);
        Self {
            enabled,
            backend,
            last_synced: None,
        }
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_backend(enabled: bool, backend: Box<dyn TrayBackend>) -> Self {
        Self {
            enabled,
            backend,
            last_synced: None,
        }
    }

    pub fn set_enabled(&mut self, enabled: bool, ctx: Option<egui::Context>) {
        if self.enabled == enabled {
            return;
        }
        self.enabled = enabled;
        self.last_synced = None;
        self.backend = create_backend(enabled, ctx);
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.enabled && self.backend.is_active()
    }

    #[must_use]
    pub fn poll_events(&self) -> Vec<TrayCommand> {
        if !self.enabled {
            return Vec::new();
        }
        self.backend.poll_events()
    }

    pub fn sync_state(&mut self, snap: &TraySnapshot) {
        if !self.enabled {
            return;
        }
        if self.last_synced.as_ref() == Some(snap) {
            return;
        }
        self.last_synced = Some(snap.clone());
        self.backend.sync_state(snap);
    }
}

fn create_backend(enabled: bool, ctx: Option<egui::Context>) -> Box<dyn TrayBackend> {
    if !enabled {
        return Box::new(NoopTrayBackend);
    }
    let Some(ctx) = ctx else {
        return Box::new(NoopTrayBackend);
    };
    #[cfg(target_os = "linux")]
    {
        if let Some(linux) = crate::tray_linux::LinuxKsniTray::spawn(ctx) {
            return Box::new(linux);
        }
    }
    #[cfg(windows)]
    {
        if let Some(win) = crate::tray_windows::WindowsTray::spawn(ctx) {
            return Box::new(win);
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    let _ = ctx;
    Box::new(NoopTrayBackend)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Default)]
    struct FakeBackend {
        active: bool,
        events: RefCell<Vec<TrayCommand>>,
        synced: Rc<RefCell<Vec<TraySnapshot>>>,
    }

    impl TrayBackend for FakeBackend {
        fn is_active(&self) -> bool {
            self.active
        }

        fn poll_events(&self) -> Vec<TrayCommand> {
            std::mem::take(&mut *self.events.borrow_mut())
        }

        fn sync_state(&mut self, snap: &TraySnapshot) {
            self.synced.borrow_mut().push(snap.clone());
        }
    }

    #[test]
    fn tray_controller_delegates_events_and_deduplicates_syncs() {
        let synced = Rc::new(RefCell::new(Vec::new()));
        let fake = FakeBackend {
            active: true,
            events: RefCell::new(vec![TrayCommand::PlayPause, TrayCommand::ToggleMiniPlayer]),
            synced: Rc::clone(&synced),
        };
        let mut ctrl = TrayController::with_backend(true, Box::new(fake));
        assert!(ctrl.is_active());
        assert_eq!(
            ctrl.poll_events(),
            vec![TrayCommand::PlayPause, TrayCommand::ToggleMiniPlayer]
        );

        let snap = TraySnapshot::default();
        ctrl.sync_state(&snap);
        ctrl.sync_state(&snap);
        assert_eq!(synced.borrow().len(), 1);

        let next = TraySnapshot {
            is_playing: true,
            ..snap
        };
        ctrl.sync_state(&next);
        assert_eq!(synced.borrow().len(), 2);
    }

    #[test]
    fn tray_icon_rgba_and_argb_conversions_produce_valid_buffers() {
        let (dark_rgba, dw, dh) = build_tray_rgba(true);
        let (light_rgba, lw, lh) = build_tray_rgba(false);
        assert!(dw >= 16 && dh >= 16);
        assert_eq!((dw, dh), (lw, lh));
        assert_eq!(dark_rgba.len(), (dw * dh * 4) as usize);
        assert_ne!(dark_rgba, light_rgba);

        let argb = rgba_to_argb32(&[10, 20, 30, 255]);
        assert_eq!(argb, vec![255, 10, 20, 30]);
    }
}
