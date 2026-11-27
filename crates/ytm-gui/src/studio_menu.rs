use eframe::egui;

use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, paint_icon};
use crate::state::GuiTab;

/// Collapsible Studio Tools section rendered in the left sidebar (Task 6.2).
pub fn show_studio_tools_menu(
    ui: &mut egui::Ui,
    lang: ResolvedLang,
    active_tab: &mut GuiTab,
    sync_len: usize,
) {
    let tools: [(GuiTab, Icon, Key); 7] = [
        (GuiTab::Dedupe, Icon::Sparkles, Key::SidebarDedupe),
        (GuiTab::Sort, Icon::Sync, Key::SidebarSort),
        (GuiTab::SetOps, Icon::GitCompare, Key::SidebarSetOps),
        (GuiTab::Health, Icon::Activity, Key::SidebarHealth),
        (GuiTab::GenreSplit, Icon::Tags, Key::SidebarGenreSplit),
        (GuiTab::Snapshots, Icon::Shield, Key::SidebarSnapshots),
        (GuiTab::Sync, Icon::Layers, Key::SidebarSync),
    ];

    egui::CollapsingHeader::new(egui::RichText::new(tr(lang, Key::SidebarStudioTools)).strong())
        .id_salt("sidebar_studio_tools_menu")
        .default_open(false)
        .show(ui, |ui| {
            for (tab, icon, key) in tools {
                let selected = *active_tab == tab;
                let text = if tab == GuiTab::Sync && sync_len > 0 {
                    format!("{} ({sync_len})", tr(lang, key))
                } else {
                    tr(lang, key).to_string()
                };
                ui.horizontal(|ui| {
                    let tint = if selected {
                        ui.visuals().selection.stroke.color
                    } else {
                        ui.visuals().text_color()
                    };
                    paint_icon(ui, icon, 14.0, tint);
                    if ui.selectable_label(selected, text).clicked() {
                        *active_tab = tab;
                    }
                });
            }
        });
}
