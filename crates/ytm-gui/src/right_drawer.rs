use eframe::egui;

use crate::app::YtmGuiApp;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, icon_text_button};
use crate::playback_ops::dispatch_play_actions;
use crate::views::queue_lyrics::{show_lyrics_section, show_queue_section};

/// Explicit state machine for the persistent right Playback Drawer (`Closed | Queue | Lyrics`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RightDrawerTab {
    #[default]
    Closed,
    Queue,
    Lyrics,
}

impl RightDrawerTab {
    /// Toggle the Queue tab (opens Queue or closes if already on Queue).
    pub fn toggle_queue(&mut self) {
        *self = match *self {
            Self::Queue => Self::Closed,
            Self::Closed | Self::Lyrics => Self::Queue,
        };
    }

    /// Toggle the Lyrics tab (opens Lyrics directly without requiring Queue, or closes if already on Lyrics).
    pub fn toggle_lyrics(&mut self) {
        *self = match *self {
            Self::Lyrics => Self::Closed,
            Self::Closed | Self::Queue => Self::Lyrics,
        };
    }

    #[must_use]
    pub const fn is_open(self) -> bool {
        !matches!(self, Self::Closed)
    }
}

/// Render the right-hand Queue / Lyrics drawer when open.
pub fn show_right_drawer(ctx: &egui::Context, app: &mut YtmGuiApp, lang: ResolvedLang) {
    if !app.right_drawer.is_open() {
        return;
    }

    let mut actions = Vec::new();
    egui::SidePanel::right("playback_right_drawer")
        .default_width(360.0)
        .min_width(280.0)
        .max_width(520.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if icon_text_button(ui, Icon::ListMusic, tr(lang, Key::PlayerQueue), 14.0, true)
                    .clicked()
                {
                    app.right_drawer = RightDrawerTab::Queue;
                }
                if icon_text_button(ui, Icon::Mic, tr(lang, Key::PlayerLyrics), 14.0, true)
                    .clicked()
                {
                    app.right_drawer = RightDrawerTab::Lyrics;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, 14.0, true, tr(lang, Key::DrawerClose))
                        .clicked()
                    {
                        app.right_drawer = RightDrawerTab::Closed;
                    }
                });
            });
            ui.separator();

            match app.right_drawer {
                RightDrawerTab::Closed => {}
                RightDrawerTab::Queue => {
                    actions = show_queue_section(ui, app);
                }
                RightDrawerTab::Lyrics => {
                    show_lyrics_section(ui, app);
                }
            }
        });

    if !actions.is_empty() {
        dispatch_play_actions(actions, app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_drawer_state_machine_switches_and_closes_independently() {
        let mut tab = RightDrawerTab::default();
        assert_eq!(tab, RightDrawerTab::Closed);
        assert!(!tab.is_open());

        // Opening Lyrics directly never requires Queue to be open first.
        tab.toggle_lyrics();
        assert_eq!(tab, RightDrawerTab::Lyrics);
        assert!(tab.is_open());

        // Switching to Queue preserves open state on Queue tab.
        tab.toggle_queue();
        assert_eq!(tab, RightDrawerTab::Queue);

        // Toggling active Queue closes the drawer.
        tab.toggle_queue();
        assert_eq!(tab, RightDrawerTab::Closed);
    }
}
