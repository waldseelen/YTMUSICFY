use crate::state::GuiTab;
use eframe::egui::{self, Color32, Stroke};

/// Five primary workspaces replacing the legacy 11-tab flat strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    HomeExplore,
    LibraryPlaylist,
    StudioTools,
    SafetyHistory,
    SettingsAuth,
}

impl Workspace {
    pub const ALL: [Self; 5] = [
        Self::HomeExplore,
        Self::LibraryPlaylist,
        Self::StudioTools,
        Self::SafetyHistory,
        Self::SettingsAuth,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::HomeExplore => "Home & Explore",
            Self::LibraryPlaylist => "Library & Playlist",
            Self::StudioTools => "Studio Tools",
            Self::SafetyHistory => "Safety & History",
            Self::SettingsAuth => "Settings & Auth",
        }
    }

    #[must_use]
    pub const fn default_tab(self) -> GuiTab {
        match self {
            Self::HomeExplore => GuiTab::Browse,
            Self::LibraryPlaylist => GuiTab::Inspector,
            Self::StudioTools => GuiTab::Dedupe,
            Self::SafetyHistory => GuiTab::Snapshots,
            Self::SettingsAuth => GuiTab::Settings,
        }
    }

    #[must_use]
    pub const fn sub_tabs(self) -> &'static [GuiTab] {
        match self {
            Self::HomeExplore => &[GuiTab::Browse, GuiTab::Search, GuiTab::QueueLyrics],
            Self::LibraryPlaylist => &[GuiTab::Inspector],
            Self::StudioTools => &[
                GuiTab::Dedupe,
                GuiTab::Sort,
                GuiTab::SetOps,
                GuiTab::Health,
                GuiTab::GenreSplit,
            ],
            Self::SafetyHistory => &[GuiTab::Snapshots, GuiTab::Sync],
            Self::SettingsAuth => &[GuiTab::Settings, GuiTab::Auth],
        }
    }

    #[must_use]
    pub const fn accent(self) -> Color32 {
        match self {
            Self::HomeExplore => Color32::from_rgb(66, 150, 210),
            Self::LibraryPlaylist => Color32::from_rgb(66, 168, 112),
            Self::StudioTools => Color32::from_rgb(206, 146, 56),
            Self::SafetyHistory => Color32::from_rgb(148, 120, 196),
            Self::SettingsAuth => Color32::from_rgb(122, 144, 178),
        }
    }

    #[must_use]
    pub const fn idle_fill(self) -> Color32 {
        match self {
            Self::HomeExplore => Color32::from_rgb(24, 34, 46),
            Self::LibraryPlaylist => Color32::from_rgb(24, 40, 32),
            Self::StudioTools => Color32::from_rgb(44, 36, 24),
            Self::SafetyHistory => Color32::from_rgb(35, 30, 46),
            Self::SettingsAuth => Color32::from_rgb(30, 34, 42),
        }
    }
}

#[must_use]
pub const fn workspace_of(tab: GuiTab) -> Workspace {
    match tab {
        GuiTab::Browse | GuiTab::Search | GuiTab::QueueLyrics => Workspace::HomeExplore,
        GuiTab::Inspector => Workspace::LibraryPlaylist,
        GuiTab::Dedupe | GuiTab::Sort | GuiTab::SetOps | GuiTab::Health | GuiTab::GenreSplit => {
            Workspace::StudioTools
        }
        GuiTab::Snapshots | GuiTab::Sync => Workspace::SafetyHistory,
        GuiTab::Auth | GuiTab::Settings => Workspace::SettingsAuth,
    }
}

#[must_use]
pub const fn tab_tooltip(tab: GuiTab) -> &'static str {
    match tab {
        GuiTab::Browse => "Browse YouTube Music Home, Explore, History, and Library Albums.",
        GuiTab::Search => {
            "Full YouTube Music catalog search across Songs, Videos, Albums, Artists, Playlists."
        }
        GuiTab::QueueLyrics => {
            "Manage playback queue, Start Radio, and inspect synced/plain lyrics."
        }
        GuiTab::Inspector => {
            "Inspect playlist tracks, sort columns, save permanent order, and open Split Diff."
        }
        GuiTab::SetOps => {
            "Compute Union, Intersection, or Difference (A - B) across two playlists."
        }
        GuiTab::Sort => "Multi-key sorting (Artist, Album, Year, Genre) using minimal LIS moves.",
        GuiTab::Dedupe => "Detect and remove duplicate tracks or redundant UGC/ATV versions.",
        GuiTab::Health => "Scan for unavailable/greyed-out tracks and upgrade UGC to Official ATV.",
        GuiTab::GenreSplit => {
            "Classify tracks via MusicBrainz/Deezer tags and split into genre playlists."
        }
        GuiTab::Snapshots => {
            "Inspect local pre-mutation SQLite backups, visual diffs, and restore any snapshot."
        }
        GuiTab::Sync => "Full-page staged mutation preview & verified POST dispatch.",
        GuiTab::Auth => {
            "Manage browser cookie import, Secret Service auth, and audio stream quality."
        }
        GuiTab::Settings => "Configure MutationPolicy, InnerTube pacing, and enrichment options.",
    }
}

/// Renders 5 Primary Workspaces + compact sub-view strip.
pub fn show_tab_bar(ui: &mut egui::Ui, active_tab: &mut GuiTab, sync_len: usize) {
    let active_ws = workspace_of(*active_tab);

    ui.horizontal_wrapped(|ui| {
        for (idx, ws) in Workspace::ALL.iter().copied().enumerate() {
            let selected = ws == active_ws;
            let fill = if selected {
                ws.accent()
            } else {
                ws.idle_fill()
            };
            let text_color = if selected {
                Color32::from_rgb(16, 18, 22)
            } else {
                Color32::from_rgb(225, 230, 238)
            };
            let stroke = if selected {
                Stroke::new(1.5_f32, Color32::WHITE)
            } else {
                Stroke::new(1.0_f32, ws.accent().gamma_multiply(0.45))
            };
            let btn = egui::Button::new(egui::RichText::new(ws.label()).color(text_color).strong())
                .fill(fill)
                .stroke(stroke)
                .corner_radius(5.0);
            let resp = ui
                .add(btn)
                .on_hover_text(format!("{} (Ctrl+{})", ws.label(), idx + 1));
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && !selected {
                *active_tab = ws.default_tab();
            }
        }
    });

    let subs = active_ws.sub_tabs();
    if subs.len() > 1 {
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            for &sub in subs {
                let selected = *active_tab == sub;
                let label = if sub == GuiTab::Sync && sync_len > 0 {
                    format!("Sync ({sync_len})")
                } else {
                    sub.label().to_string()
                };
                if ui
                    .selectable_label(selected, label)
                    .on_hover_text(tab_tooltip(sub))
                    .clicked()
                {
                    *active_tab = sub;
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_gui_tab_maps_to_a_workspace_and_sub_tab() {
        for tab in GuiTab::ALL {
            assert!(!tab_tooltip(tab).trim().is_empty());
            let ws = workspace_of(tab);
            assert!(
                ws.sub_tabs().contains(&tab),
                "tab {tab:?} missing from workspace {ws:?} sub_tabs"
            );
        }
    }
}
