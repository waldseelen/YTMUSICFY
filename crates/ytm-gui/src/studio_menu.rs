use eframe::egui;

use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, paint_icon};
use crate::state::GuiTab;
use crate::tab_bar::{Workspace, workspace_of};

/// Primary Apple Music / Linear sidebar destinations replacing duplicate top & accordion navs.
pub fn show_primary_sidebar_nav(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    active_tab: &mut GuiTab,
    sync_len: usize,
) {
    let active_ws = workspace_of(*active_tab);
    let nav_items: [(Workspace, Icon, String); 4] = [
        (
            Workspace::HomeExplore,
            Icon::Globe,
            Workspace::HomeExplore.label().to_string(),
        ),
        (
            Workspace::StudioTools,
            Icon::Sparkles,
            Workspace::StudioTools.label().to_string(),
        ),
        (
            Workspace::SafetyHistory,
            Icon::Shield,
            if sync_len > 0 {
                format!("{} ({sync_len})", Workspace::SafetyHistory.label())
            } else {
                Workspace::SafetyHistory.label().to_string()
            },
        ),
        (
            Workspace::SettingsAuth,
            Icon::Settings,
            tr(lang, Key::SidebarSettings).to_string(),
        ),
    ];

    for (ws, icon, label) in nav_items {
        let selected = active_ws == ws;
        let tint = if selected {
            ui.visuals().selection.stroke.color
        } else {
            ui.visuals().text_color()
        };
        ui.horizontal(|ui| {
            paint_icon(ui, icon, 14.0, tint);
            if ui.selectable_label(selected, label).clicked() {
                *active_tab = ws.default_tab();
            }
        });
    }
}
