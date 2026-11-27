use std::sync::mpsc::Sender;

use eframe::egui;
use ytm_core::SearchFilter;

use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, paint_icon};
use crate::playback_ops::PlayAction;
use crate::search_controller::SearchController;
use crate::state::{GuiTab, WorkerCmd};
use crate::theme::Palette;

#[derive(Clone)]
enum DropdownItem {
    Query(String),
    Track(ytm_core::Track),
    Browse(String),
}

pub struct SearchDropdownCtx<'a> {
    pub anchor_rect: egui::Rect,
    pub ctrl: &'a mut SearchController,
    pub palette: &'a Palette,
    pub lang: ResolvedLang,
    pub catalog_query: &'a mut String,
    pub catalog_filter: SearchFilter,
    pub active_tab: &'a mut GuiTab,
    pub recent_persisted: &'a mut Vec<String>,
    pub cmd_tx: &'a Sender<WorkerCmd>,
}

/// Render the instant search dropdown anchored directly below the top-bar search input.
pub fn show_search_dropdown(ctx: &egui::Context, dd: SearchDropdownCtx<'_>) -> Vec<PlayAction> {
    let SearchDropdownCtx {
        anchor_rect,
        ctrl,
        palette,
        lang,
        catalog_query,
        catalog_filter,
        active_tab,
        recent_persisted,
        cmd_tx,
    } = dd;
    let mut actions = Vec::new();
    if !ctrl.dropdown_open || !anchor_rect.is_positive() {
        return actions;
    }

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        ctrl.dropdown_open = false;
        ctrl.highlighted_idx = None;
        return actions;
    }

    let mut entries: Vec<(String, String, DropdownItem)> = Vec::new();
    if ctrl.query.trim().is_empty() {
        for q in ctrl.recent_queries.iter().take(6) {
            entries.push((
                tr(lang, Key::SearchRecent).to_string(),
                q.clone(),
                DropdownItem::Query(q.clone()),
            ));
        }
    } else {
        for s in ctrl.suggestions.iter().take(4) {
            entries.push((
                tr(lang, Key::SearchSuggestions).to_string(),
                s.clone(),
                DropdownItem::Query(s.clone()),
            ));
        }
        let preview = ctrl.grouped_preview();
        if let Some(card) = preview.top_card {
            if let Some(bid) = &card.browse_id {
                entries.push((
                    tr(lang, Key::SearchTopMatch).to_string(),
                    format!("{} — {}", card.title, card.subtitle),
                    DropdownItem::Browse(bid.clone()),
                ));
            }
        } else if let Some(track) = preview.top_track {
            entries.push((
                tr(lang, Key::SearchTopMatch).to_string(),
                format!(
                    "{} — {}",
                    track.title,
                    track.primary_artist_name().unwrap_or("-")
                ),
                DropdownItem::Track(track.clone()),
            ));
        }
        for track in preview.songs {
            entries.push((
                tr(lang, Key::SearchSongs).to_string(),
                format!(
                    "{} — {}",
                    track.title,
                    track.primary_artist_name().unwrap_or("-")
                ),
                DropdownItem::Track(track.clone()),
            ));
        }
        if let Some(artist) = preview.artist
            && let Some(bid) = &artist.browse_id
        {
            entries.push((
                tr(lang, Key::SearchArtist).to_string(),
                artist.title.clone(),
                DropdownItem::Browse(bid.clone()),
            ));
        }
        if let Some(album) = preview.album
            && let Some(bid) = &album.browse_id
        {
            entries.push((
                tr(lang, Key::SearchAlbum).to_string(),
                format!("{} — {}", album.title, album.subtitle),
                DropdownItem::Browse(bid.clone()),
            ));
        }
    }

    if entries.is_empty() {
        return actions;
    }

    let (down, up, enter) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowDown),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::Enter),
        )
    });
    if down {
        ctrl.highlighted_idx = Some(ctrl.highlighted_idx.map_or(0, |i| (i + 1) % entries.len()));
    } else if up {
        ctrl.highlighted_idx = Some(match ctrl.highlighted_idx {
            Some(0) | None => entries.len() - 1,
            Some(i) => i - 1,
        });
    }

    let mut chosen: Option<DropdownItem> = None;
    if enter
        && let Some(idx) = ctrl
            .highlighted_idx
            .and_then(|i| entries.get(i).map(|e| e.2.clone()))
    {
        chosen = Some(idx);
    }

    let pos = egui::pos2(anchor_rect.left(), anchor_rect.bottom() + 4.0);
    let area_resp = egui::Area::new(egui::Id::new("dalga_instant_search_dropdown"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(palette.bg_panel)
                .stroke(egui::Stroke::new(1.0_f32, palette.border_subtle))
                .corner_radius(6.0)
                .inner_margin(8.0)
                .show(ui, |ui| {
                    ui.set_width(anchor_rect.width().max(340.0));
                    let mut last_group = String::new();
                    for (idx, (group, label, item)) in entries.iter().enumerate() {
                        if *group != last_group {
                            if !last_group.is_empty() {
                                ui.separator();
                            }
                            ui.colored_label(
                                palette.text_muted,
                                egui::RichText::new(group).small().strong(),
                            );
                            last_group = group.clone();
                        }
                        let selected = ctrl.highlighted_idx == Some(idx);
                        let icon = match item {
                            DropdownItem::Query(_) => Icon::Search,
                            DropdownItem::Track(_) => Icon::Play,
                            DropdownItem::Browse(_) => Icon::ExternalLink,
                        };
                        ui.horizontal(|ui| {
                            paint_icon(ui, icon, 13.0, palette.text_muted);
                            if ui.selectable_label(selected, label).clicked() {
                                chosen = Some(item.clone());
                            }
                        });
                    }
                    ui.separator();
                    ui.colored_label(
                        palette.text_muted,
                        egui::RichText::new(tr(lang, Key::SearchEnterHint)).small(),
                    );
                });
        })
        .response;

    if let Some(item) = chosen {
        ctrl.dropdown_open = false;
        ctrl.highlighted_idx = None;
        match item {
            DropdownItem::Query(q) => {
                ctrl.query = q.clone();
                *recent_persisted = ctrl.record_recent(&q);
                *catalog_query = q.clone();
                *active_tab = GuiTab::Search;
                let _ = cmd_tx.send(WorkerCmd::SearchCatalog {
                    query: q,
                    filter: catalog_filter,
                    continuation: None,
                });
            }
            DropdownItem::Track(t) => {
                *recent_persisted = ctrl.record_recent(&ctrl.query.clone());
                actions.push(PlayAction::PlayFromList {
                    tracks: vec![t],
                    start_idx: 0,
                });
            }
            DropdownItem::Browse(bid) => {
                *recent_persisted = ctrl.record_recent(&ctrl.query.clone());
                actions.push(PlayAction::OpenBrowse(bid));
            }
        }
        return actions;
    }

    if ctx.input(|i| i.pointer.any_pressed())
        && let Some(ptr) = ctx.pointer_interact_pos()
        && !area_resp.rect.contains(ptr)
        && !anchor_rect.contains(ptr)
    {
        ctrl.dropdown_open = false;
        ctrl.highlighted_idx = None;
    }

    actions
}
