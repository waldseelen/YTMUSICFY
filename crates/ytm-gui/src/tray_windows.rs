use std::sync::mpsc;

use eframe::egui;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::tray::{TrayBackend, TrayCommand, TraySnapshot, build_tray_rgba, menu_labels};

/// Windows native Win32 system tray implementation via `tray-icon` + `muda`.
pub struct WindowsTray {
    tray_icon: TrayIcon,
    cmd_rx: mpsc::Receiver<TrayCommand>,
    show_hide_item: MenuItem,
    play_pause_item: MenuItem,
    prev_item: MenuItem,
    next_item: MenuItem,
    mini_item: MenuItem,
    quit_item: MenuItem,
    last_dark: Option<bool>,
    last_tooltip: Option<String>,
}

impl WindowsTray {
    pub fn spawn(egui_ctx: egui::Context) -> Option<Self> {
        let labels = menu_labels(&TraySnapshot::default());
        let show_hide_item =
            MenuItem::with_id("dalga_tray_show_hide", labels.show_hide, true, None);
        let play_pause_item =
            MenuItem::with_id("dalga_tray_play_pause", labels.play_pause, true, None);
        let prev_item = MenuItem::with_id("dalga_tray_prev", labels.prev, true, None);
        let next_item = MenuItem::with_id("dalga_tray_next", labels.next, true, None);
        let mini_item = MenuItem::with_id("dalga_tray_mini", labels.mini_player, true, None);
        let quit_item = MenuItem::with_id("dalga_tray_quit", labels.quit, true, None);

        let menu = Menu::new();
        let sep1 = PredefinedMenuItem::separator();
        let sep2 = PredefinedMenuItem::separator();
        let _ = menu.append_items(&[
            &show_hide_item,
            &sep1,
            &play_pause_item,
            &prev_item,
            &next_item,
            &mini_item,
            &sep2,
            &quit_item,
        ]);

        let icon = build_win_icon(true)?;
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(crate::brand::TRAY_TITLE)
            .with_icon(icon)
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        let (cmd_tx, cmd_rx) = mpsc::channel::<TrayCommand>();
        let tx_click = cmd_tx.clone();
        let ctx_click = egui_ctx.clone();
        TrayIconEvent::set_event_handler(Some(move |ev: TrayIconEvent| {
            if matches!(
                ev,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                let _ = tx_click.send(TrayCommand::ToggleWindow);
                ctx_click.request_repaint();
            }
        }));

        MenuEvent::set_event_handler(Some(move |ev: MenuEvent| {
            if let Some(cmd) = map_menu_id(&ev.id) {
                let _ = cmd_tx.send(cmd);
                egui_ctx.request_repaint();
            }
        }));

        Some(Self {
            tray_icon,
            cmd_rx,
            show_hide_item,
            play_pause_item,
            prev_item,
            next_item,
            mini_item,
            quit_item,
            last_dark: Some(true),
            last_tooltip: None,
        })
    }
}

fn map_menu_id(id: &MenuId) -> Option<TrayCommand> {
    match id.0.as_str() {
        "dalga_tray_show_hide" => Some(TrayCommand::ToggleWindow),
        "dalga_tray_play_pause" => Some(TrayCommand::PlayPause),
        "dalga_tray_prev" => Some(TrayCommand::Previous),
        "dalga_tray_next" => Some(TrayCommand::Next),
        "dalga_tray_mini" => Some(TrayCommand::ToggleMiniPlayer),
        "dalga_tray_quit" => Some(TrayCommand::Quit),
        _ => None,
    }
}

fn build_win_icon(dark_theme: bool) -> Option<Icon> {
    let (rgba, w, h) = build_tray_rgba(dark_theme);
    Icon::from_rgba(rgba, w, h).ok()
}

impl TrayBackend for WindowsTray {
    fn is_active(&self) -> bool {
        true
    }

    fn poll_events(&self) -> Vec<TrayCommand> {
        let mut out = Vec::new();
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            out.push(cmd);
        }
        out
    }

    fn sync_state(&mut self, snap: &TraySnapshot) {
        let labels = menu_labels(snap);
        self.show_hide_item.set_text(labels.show_hide);
        self.play_pause_item.set_text(labels.play_pause);
        self.prev_item.set_text(labels.prev);
        self.next_item.set_text(labels.next);
        self.mini_item.set_text(labels.mini_player);
        self.quit_item.set_text(labels.quit);

        if self.last_dark != Some(snap.dark_theme) {
            self.last_dark = Some(snap.dark_theme);
            if let Some(icon) = build_win_icon(snap.dark_theme) {
                let _ = self.tray_icon.set_icon(Some(icon));
            }
        }
        let tip = snap
            .track_label
            .clone()
            .unwrap_or_else(|| crate::brand::TRAY_TITLE.to_string());
        if self.last_tooltip.as_deref() != Some(&tip) {
            self.last_tooltip = Some(tip.clone());
            let _ = self.tray_icon.set_tooltip(Some(tip));
        }
    }
}
