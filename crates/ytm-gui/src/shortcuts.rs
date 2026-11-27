use eframe::egui;

use crate::i18n::{Key, ResolvedLang, tr};

/// Functional grouping for the Shortcuts registry and Settings shortcuts table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutCategory {
    Playback,
    Navigation,
    Search,
    Editing,
    Window,
}

impl ShortcutCategory {
    #[must_use]
    pub const fn label(self, lang: ResolvedLang) -> &'static str {
        match (self, lang) {
            (Self::Playback, ResolvedLang::En) => "Playback",
            (Self::Playback, ResolvedLang::Tr) => "Oynatma",
            (Self::Navigation, ResolvedLang::En) => "Navigation",
            (Self::Navigation, ResolvedLang::Tr) => "Gezinme",
            (Self::Search, ResolvedLang::En) => "Search",
            (Self::Search, ResolvedLang::Tr) => "Arama",
            (Self::Editing, ResolvedLang::En) => "Editing & Sync",
            (Self::Editing, ResolvedLang::Tr) => "Duzenleme ve Esitleme",
            (Self::Window, ResolvedLang::En) => "Window",
            (Self::Window, ResolvedLang::Tr) => "Pencere",
        }
    }
}

/// Canonical keyboard or OS media shortcut entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutSpec {
    pub id: &'static str,
    pub binding: &'static str,
    pub category: ShortcutCategory,
    pub label_key: Key,
}

/// Single source of truth for all keyboard and OS media shortcuts in Dalga.
pub const SHORTCUTS: &[ShortcutSpec] = &[
    ShortcutSpec {
        id: "play_pause",
        binding: "Space",
        category: ShortcutCategory::Playback,
        label_key: Key::ShortcutPlayPause,
    },
    ShortcutSpec {
        id: "mute",
        binding: "M",
        category: ShortcutCategory::Playback,
        label_key: Key::ShortcutMute,
    },
    ShortcutSpec {
        id: "os_media_keys",
        binding: "Media Play/Next/Prev/Stop",
        category: ShortcutCategory::Playback,
        label_key: Key::ShortcutMediaKeys,
    },
    ShortcutSpec {
        id: "focus_search_slash",
        binding: "/",
        category: ShortcutCategory::Search,
        label_key: Key::ShortcutFocusSearchSlash,
    },
    ShortcutSpec {
        id: "focus_search_ctrl_f",
        binding: "Ctrl+F",
        category: ShortcutCategory::Search,
        label_key: Key::ShortcutFocusSearch,
    },
    ShortcutSpec {
        id: "command_palette",
        binding: "Ctrl+K",
        category: ShortcutCategory::Navigation,
        label_key: Key::ShortcutCommandPalette,
    },
    ShortcutSpec {
        id: "workspaces",
        binding: "Ctrl+1 .. Ctrl+5",
        category: ShortcutCategory::Navigation,
        label_key: Key::ShortcutWorkspaces,
    },
    ShortcutSpec {
        id: "history_nav",
        binding: "Alt+Left / Alt+Right",
        category: ShortcutCategory::Navigation,
        label_key: Key::ShortcutBackForward,
    },
    ShortcutSpec {
        id: "arrow_nav",
        binding: "Up / Down",
        category: ShortcutCategory::Navigation,
        label_key: Key::ShortcutArrowNav,
    },
    ShortcutSpec {
        id: "open_ytm",
        binding: "Ctrl+O",
        category: ShortcutCategory::Navigation,
        label_key: Key::ShortcutOpenYtm,
    },
    ShortcutSpec {
        id: "open_ytm_time",
        binding: "Ctrl+Shift+O",
        category: ShortcutCategory::Navigation,
        label_key: Key::ShortcutOpenYtmTime,
    },
    ShortcutSpec {
        id: "sync_drawer",
        binding: "Ctrl+Shift+S",
        category: ShortcutCategory::Editing,
        label_key: Key::ShortcutSyncDrawer,
    },
    ShortcutSpec {
        id: "undo_redo",
        binding: "Ctrl+Z / Ctrl+Shift+Z",
        category: ShortcutCategory::Editing,
        label_key: Key::ShortcutUndoRedo,
    },
    ShortcutSpec {
        id: "execute_sync",
        binding: "Ctrl+Enter",
        category: ShortcutCategory::Editing,
        label_key: Key::ShortcutExecuteSync,
    },
    ShortcutSpec {
        id: "toggle_mini_player",
        binding: "Ctrl+M",
        category: ShortcutCategory::Window,
        label_key: Key::ShortcutMiniPlayer,
    },
    ShortcutSpec {
        id: "zoom",
        binding: "Ctrl++ / Ctrl+- / Ctrl+0",
        category: ShortcutCategory::Window,
        label_key: Key::ShortcutZoom,
    },
];

/// Materialize localized `(category, binding, description)` rows directly from `SHORTCUTS`.
#[must_use]
pub fn shortcut_rows(lang: ResolvedLang) -> Vec<(&'static str, &'static str, &'static str)> {
    SHORTCUTS
        .iter()
        .map(|s| (s.category.label(lang), s.binding, tr(lang, s.label_key)))
        .collect()
}

/// Render the read-only Shortcuts table generated directly from `SHORTCUTS`.
pub fn show_shortcuts_table(ui: &mut egui::Ui, lang: ResolvedLang) {
    ui.strong(tr(lang, Key::SettingsShortcutsHeading));
    egui::Grid::new("dalga_shortcuts_registry_grid")
        .striped(true)
        .spacing([14.0, 6.0])
        .show(ui, |ui| {
            for (category, binding, desc) in shortcut_rows(lang) {
                ui.weak(category);
                ui.monospace(binding);
                ui.label(desc);
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn shortcut_registry_has_no_duplicate_bindings_or_ids() {
        let mut bindings = HashSet::new();
        let mut ids = HashSet::new();
        for spec in SHORTCUTS {
            assert!(!spec.id.trim().is_empty());
            assert!(!spec.binding.trim().is_empty());
            assert!(ids.insert(spec.id), "duplicate shortcut id: {}", spec.id);
            assert!(
                bindings.insert(spec.binding),
                "duplicate shortcut binding: {}",
                spec.binding
            );
        }
    }

    #[test]
    fn shortcuts_page_rows_match_registry_one_to_one() {
        for lang in [ResolvedLang::En, ResolvedLang::Tr] {
            let rows = shortcut_rows(lang);
            assert_eq!(rows.len(), SHORTCUTS.len());
            for (row, spec) in rows.iter().zip(SHORTCUTS.iter()) {
                assert_eq!(row.0, spec.category.label(lang));
                assert_eq!(row.1, spec.binding);
                assert_eq!(row.2, tr(lang, spec.label_key));
            }
        }
    }
}
