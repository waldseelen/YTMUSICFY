use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};

use eframe::egui;
use ksni::menu::StandardItem;
use ksni::{Category, Icon, MenuItem, Status, ToolTip, Tray, TrayMethods};
use tokio::sync::mpsc as tokio_mpsc;

use crate::tray::{
    TrayBackend, TrayCommand, TraySnapshot, build_tray_rgba, menu_labels, rgba_to_argb32,
};

struct DalgaKsniTray {
    snapshot: TraySnapshot,
    cmd_tx: mpsc::Sender<TrayCommand>,
    egui_ctx: egui::Context,
    dark_icon: Icon,
    light_icon: Icon,
}

impl DalgaKsniTray {
    fn emit(&self, cmd: TrayCommand) {
        let _ = self.cmd_tx.send(cmd);
        self.egui_ctx.request_repaint();
    }
}

impl Tray for DalgaKsniTray {
    fn id(&self) -> String {
        "dalga".to_string()
    }

    fn category(&self) -> Category {
        Category::ApplicationStatus
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn title(&self) -> String {
        crate::brand::TRAY_TITLE.to_string()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        if self.snapshot.dark_theme {
            vec![self.dark_icon.clone()]
        } else {
            vec![self.light_icon.clone()]
        }
    }

    fn tool_tip(&self) -> ToolTip {
        let description = self
            .snapshot
            .track_label
            .clone()
            .unwrap_or_else(|| crate::brand::TAGLINE.to_string());
        ToolTip {
            title: crate::brand::TRAY_TITLE.to_string(),
            description,
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.emit(TrayCommand::ToggleWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let labels = menu_labels(&self.snapshot);
        vec![
            StandardItem {
                label: labels.show_hide.to_string(),
                activate: Box::new(|this: &mut Self| this.emit(TrayCommand::ToggleWindow)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: labels.play_pause.to_string(),
                activate: Box::new(|this: &mut Self| this.emit(TrayCommand::PlayPause)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: labels.prev.to_string(),
                activate: Box::new(|this: &mut Self| this.emit(TrayCommand::Previous)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: labels.next.to_string(),
                activate: Box::new(|this: &mut Self| this.emit(TrayCommand::Next)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: labels.mini_player.to_string(),
                activate: Box::new(|this: &mut Self| this.emit(TrayCommand::ToggleMiniPlayer)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: labels.quit.to_string(),
                activate: Box::new(|this: &mut Self| this.emit(TrayCommand::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn build_ksni_icon(dark_theme: bool) -> Icon {
    let (rgba, w, h) = build_tray_rgba(dark_theme);
    Icon {
        width: w as i32,
        height: h as i32,
        data: rgba_to_argb32(&rgba),
    }
}

enum TrayControlMsg {
    Update(TraySnapshot),
    Shutdown,
}

/// Linux StatusNotifierItem (`ksni`) tray implementation running on an isolated Tokio thread.
pub struct LinuxKsniTray {
    cmd_rx: mpsc::Receiver<TrayCommand>,
    ctrl_tx: tokio_mpsc::UnboundedSender<TrayControlMsg>,
    active: Arc<AtomicBool>,
}

impl LinuxKsniTray {
    pub fn spawn(egui_ctx: egui::Context) -> Option<Self> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<TrayCommand>();
        let (ctrl_tx, mut ctrl_rx) = tokio_mpsc::unbounded_channel::<TrayControlMsg>();
        let (ready_tx, ready_rx) = mpsc::channel::<bool>();
        let active = Arc::new(AtomicBool::new(false));
        let active_clone = Arc::clone(&active);

        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                let _ = ready_tx.send(false);
                return;
            };
            rt.block_on(async move {
                let tray = DalgaKsniTray {
                    snapshot: TraySnapshot::default(),
                    cmd_tx,
                    egui_ctx,
                    dark_icon: build_ksni_icon(true),
                    light_icon: build_ksni_icon(false),
                };
                let handle = match tray.spawn().await {
                    Ok(h) => {
                        active_clone.store(true, Ordering::Release);
                        let _ = ready_tx.send(true);
                        h
                    }
                    Err(err) => {
                        tracing::debug!("ksni tray unavailable: {err}");
                        let _ = ready_tx.send(false);
                        return;
                    }
                };
                while let Some(msg) = ctrl_rx.recv().await {
                    match msg {
                        TrayControlMsg::Update(snap) => {
                            let _ = handle.update(move |t| t.snapshot = snap).await;
                        }
                        TrayControlMsg::Shutdown => {
                            handle.shutdown().await;
                            break;
                        }
                    }
                }
                active_clone.store(false, Ordering::Release);
            });
        });

        let ok = ready_rx
            .recv_timeout(std::time::Duration::from_millis(800))
            .unwrap_or(false);
        if ok {
            Some(Self {
                cmd_rx,
                ctrl_tx,
                active,
            })
        } else {
            None
        }
    }
}

impl TrayBackend for LinuxKsniTray {
    fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    fn poll_events(&self) -> Vec<TrayCommand> {
        let mut out = Vec::new();
        while let Ok(cmd) = self.cmd_rx.try_recv() {
            out.push(cmd);
        }
        out
    }

    fn sync_state(&mut self, snap: &TraySnapshot) {
        let _ = self.ctrl_tx.send(TrayControlMsg::Update(snap.clone()));
    }
}

impl Drop for LinuxKsniTray {
    fn drop(&mut self) {
        let _ = self.ctrl_tx.send(TrayControlMsg::Shutdown);
    }
}
