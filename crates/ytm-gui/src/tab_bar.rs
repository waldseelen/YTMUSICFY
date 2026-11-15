use eframe::egui::{self, Color32, Stroke};

use crate::state::GuiTab;

/// Logical functional groups sharing cohesive visual color families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabGroup {
    Discovery,
    Workspace,
    Maintenance,
    System,
}

impl TabGroup {
    #[must_use]
    pub const fn accent(self) -> Color32 {
        match self {
            Self::Discovery => Color32::from_rgb(66, 150, 210),
            Self::Workspace => Color32::from_rgb(66, 168, 112),
            Self::Maintenance => Color32::from_rgb(206, 146, 56),
            Self::System => Color32::from_rgb(148, 120, 196),
        }
    }

    #[must_use]
    pub const fn idle_fill(self) -> Color32 {
        match self {
            Self::Discovery => Color32::from_rgb(27, 42, 54),
            Self::Workspace => Color32::from_rgb(27, 46, 36),
            Self::Maintenance => Color32::from_rgb(49, 39, 25),
            Self::System => Color32::from_rgb(39, 34, 51),
        }
    }
}

#[must_use]
pub const fn tab_group(tab: GuiTab) -> TabGroup {
    match tab {
        GuiTab::Browse | GuiTab::QueueLyrics => TabGroup::Discovery,
        GuiTab::Inspector | GuiTab::SetOps => TabGroup::Workspace,
        GuiTab::Sort | GuiTab::Dedupe | GuiTab::Health | GuiTab::GenreSplit => {
            TabGroup::Maintenance
        }
        GuiTab::Snapshots | GuiTab::Sync | GuiTab::Auth => TabGroup::System,
    }
}

#[must_use]
pub const fn tab_tooltip(tab: GuiTab) -> &'static str {
    match tab {
        GuiTab::Browse => "Browse YouTube Music home shelves, charts, and recommendations.",
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
        GuiTab::Snapshots => "Inspect local pre-mutation SQLite backups and restore any snapshot.",
        GuiTab::Sync => "Preview staged mutations and execute them safely to YouTube Music.",
        GuiTab::Auth => {
            "Manage browser cookie import, Secret Service auth, and audio stream quality."
        }
    }
}

const WIDE_ROW_BREAKPOINT: f32 = 960.0;
const ROW_TOP: [GuiTab; 5] = [
    GuiTab::Browse,
    GuiTab::QueueLyrics,
    GuiTab::Inspector,
    GuiTab::SetOps,
    GuiTab::Snapshots,
];
const ROW_BOTTOM: [GuiTab; 6] = [
    GuiTab::Sort,
    GuiTab::Dedupe,
    GuiTab::Health,
    GuiTab::GenreSplit,
    GuiTab::Sync,
    GuiTab::Auth,
];

/// Renders grouped, color-coded navigation tabs with hover tooltips and balanced 2-row fallback.
pub fn show_tab_bar(ui: &mut egui::Ui, active_tab: &mut GuiTab, sync_len: usize) {
    if ui.available_width() >= WIDE_ROW_BREAKPOINT {
        ui.horizontal(|ui| {
            let mut prev_group = None;
            for tab in GuiTab::ALL {
                let grp = tab_group(tab);
                if prev_group.is_some_and(|g| g != grp) {
                    ui.separator();
                }
                prev_group = Some(grp);
                draw_tab_pill(ui, tab, active_tab, sync_len, false);
            }
        });
    } else {
        draw_equal_row(ui, &ROW_TOP, active_tab, sync_len);
        ui.add_space(3.0);
        draw_equal_row(ui, &ROW_BOTTOM, active_tab, sync_len);
    }
}

fn draw_equal_row(ui: &mut egui::Ui, tabs: &[GuiTab], active_tab: &mut GuiTab, sync_len: usize) {
    ui.columns(tabs.len(), |cols| {
        for (col_ui, &tab) in cols.iter_mut().zip(tabs.iter()) {
            draw_tab_pill(col_ui, tab, active_tab, sync_len, true);
        }
    });
}

fn draw_tab_pill(
    ui: &mut egui::Ui,
    tab: GuiTab,
    active_tab: &mut GuiTab,
    sync_len: usize,
    fill_width: bool,
) {
    let selected = *active_tab == tab;
    let group = tab_group(tab);
    let accent = group.accent();
    let label = if tab == GuiTab::Sync && sync_len > 0 {
        format!("Sync ({sync_len})")
    } else {
        tab.label().to_string()
    };

    let fill = if selected {
        accent
    } else if tab == GuiTab::Sync && sync_len > 0 {
        Color32::from_rgb(66, 52, 88)
    } else {
        group.idle_fill()
    };
    let text_color = if selected {
        Color32::from_rgb(16, 18, 22)
    } else {
        Color32::from_rgb(225, 230, 238)
    };
    let stroke = if selected {
        Stroke::new(1.5_f32, Color32::WHITE)
    } else {
        Stroke::new(1.0_f32, accent.gamma_multiply(0.45))
    };

    let mut btn = egui::Button::new(egui::RichText::new(label).color(text_color).strong())
        .fill(fill)
        .stroke(stroke)
        .corner_radius(4.0);
    if fill_width {
        btn = btn.min_size(egui::vec2(ui.available_width(), 24.0));
    }
    let resp = ui.add(btn).on_hover_text(tab_tooltip(tab));
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        *active_tab = tab;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tabs_have_tooltips_and_unique_two_row_assignment() {
        for tab in GuiTab::ALL {
            assert!(!tab_tooltip(tab).trim().is_empty());
            let in_top = ROW_TOP.contains(&tab);
            let in_bottom = ROW_BOTTOM.contains(&tab);
            assert!(in_top ^ in_bottom, "tab {tab:?} must be in exactly one row");
        }
        assert_eq!(ROW_TOP.len() + ROW_BOTTOM.len(), GuiTab::ALL.len());
    }
}
