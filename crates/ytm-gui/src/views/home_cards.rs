use eframe::egui;
use ytm_core::{BrowseCard, BrowseCardKind, PlaylistSummary};

use crate::artwork::ArtworkCache;
use crate::i18n::{Key, ResolvedLang, tr};
use crate::icons::{Icon, icon_button, icon_text_button};
use crate::navigation::ToastState;
use crate::play_queue::PlayQueue;
use crate::playback_ops::PlayAction;
use crate::sync_queue::SyncQueue;
use crate::views::context_menu::{ContextMenuEnv, ContextTarget, attach_context_menu};

/// Shared mutable rendering environment for Home shelves, cards, and track rows.
pub(crate) struct ShelfUiEnv<'a> {
    pub lang: ResolvedLang,
    pub playlists: &'a [PlaylistSummary],
    pub sync_queue: &'a mut SyncQueue,
    pub artwork: &'a mut ArtworkCache,
    pub toast: &'a mut Option<ToastState>,
    pub actions: &'a mut Vec<PlayAction>,
}

/// Render the "Continue Listening" card from the persisted/active playback session.
pub(crate) fn render_continue_card(ui: &mut egui::Ui, queue: &PlayQueue, env: &mut ShelfUiEnv<'_>) {
    let Some(track) = queue.current_track() else {
        return;
    };
    let idx = queue.current_index().unwrap_or(0);
    let total = queue.tracks().len();
    let lang = env.lang;
    let resp = ui
        .group(|ui| {
            ui.horizontal(|ui| {
                if let Some(url) = track.effective_thumbnail_url() {
                    paint_art_box(ui, &url, egui::vec2(58.0, 58.0), env.artwork);
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(58.0, 58.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 6.0, ui.visuals().widgets.inactive.bg_fill);
                }
                ui.vertical(|ui| {
                    ui.small(
                        egui::RichText::new(tr(lang, Key::HomeContinueListening))
                            .color(ui.visuals().selection.stroke.color)
                            .strong(),
                    );
                    ui.strong(&track.title);
                    ui.small(format!(
                        "{} • {}/{}",
                        track.primary_artist_name().unwrap_or("-"),
                        idx + 1,
                        total.max(1)
                    ));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_text_button(ui, Icon::Play, tr(lang, Key::HomeResume), 14.0, true)
                        .clicked()
                    {
                        env.actions.push(PlayAction::PlayFromList {
                            tracks: queue.tracks().to_vec(),
                            start_idx: idx,
                        });
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click());

    attach_context_menu(
        &resp,
        ContextTarget::Track {
            track: track.clone(),
            queue_list: queue.tracks().to_vec(),
            row_idx: idx,
        },
        ContextMenuEnv {
            lang,
            playlists: env.playlists,
            sync_queue: env.sync_queue,
            toast: env.toast,
            actions: env.actions,
        },
        None,
    );
    ui.add_space(6.0);
}

/// Render clean horizontal skeleton card shelves while Home/Browse data is loading.
pub(crate) fn render_skeleton_shelves(ui: &mut egui::Ui, msg: &str) {
    ui.horizontal(|ui| {
        ui.spinner();
        ui.weak(msg);
    });
    ui.add_space(6.0);
    let fill = ui.visuals().widgets.inactive.bg_fill;
    for shelf_idx in 0..2 {
        ui.group(|ui| {
            let (title_rect, _) =
                ui.allocate_exact_size(egui::vec2(160.0, 16.0), egui::Sense::hover());
            ui.painter().rect_filled(title_rect, 4.0, fill);
            ui.add_space(6.0);
            egui::ScrollArea::horizontal()
                .id_salt(("home_skeleton_shelf", shelf_idx))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for _ in 0..5 {
                            ui.group(|ui| {
                                ui.set_width(142.0);
                                let (cover, _) = ui.allocate_exact_size(
                                    egui::vec2(124.0, 124.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(cover, 4.0, fill);
                                ui.add_space(4.0);
                                let (l1, _) = ui.allocate_exact_size(
                                    egui::vec2(108.0, 12.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(l1, 3.0, fill);
                                let (l2, _) = ui.allocate_exact_size(
                                    egui::vec2(76.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(l2, 3.0, fill);
                            });
                        }
                    });
                });
        });
        ui.add_space(6.0);
    }
}

/// Render a single horizontal-shelf card with artwork, action buttons, and shared context menu.
pub(crate) fn render_shelf_card(ui: &mut egui::Ui, card: &BrowseCard, env: &mut ShelfUiEnv<'_>) {
    let lang = env.lang;
    let resp = ui
        .group(|ui| {
            ui.set_width(144.0);
            ui.vertical(|ui| {
                if let Some(url) = &card.thumbnail_url {
                    paint_art_box(ui, url, egui::vec2(126.0, 126.0), env.artwork);
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(126.0, 126.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 4.0, ui.visuals().widgets.inactive.bg_fill);
                }
                ui.add(egui::Label::new(egui::RichText::new(&card.title).strong()).truncate());
                ui.add(egui::Label::new(egui::RichText::new(&card.subtitle).small()).truncate());
                ui.horizontal(|ui| match card.kind {
                    BrowseCardKind::Track => {
                        if let Some(ContextTarget::Track { track, .. }) =
                            ContextTarget::from_browse_card(card)
                        {
                            if icon_button(ui, Icon::Play, 12.0, true, tr(lang, Key::CtxPlayNow))
                                .clicked()
                            {
                                env.actions.push(PlayAction::PlayFromList {
                                    tracks: vec![track.clone()],
                                    start_idx: 0,
                                });
                            }
                            if ui.small_button("+Q").clicked() {
                                env.actions.push(PlayAction::AppendQueue(track));
                            }
                        }
                    }
                    BrowseCardKind::Artist | BrowseCardKind::Album | BrowseCardKind::Playlist => {
                        if let Some(bid) = &card.browse_id
                            && ui.small_button("Open ->").clicked()
                        {
                            env.actions.push(PlayAction::OpenBrowse(bid.clone()));
                        }
                    }
                });
            });
        })
        .response
        .interact(egui::Sense::click());

    if resp.clicked() {
        match card.kind {
            BrowseCardKind::Track => {
                if let Some(ContextTarget::Track { track, .. }) =
                    ContextTarget::from_browse_card(card)
                {
                    env.actions.push(PlayAction::PlayFromList {
                        tracks: vec![track],
                        start_idx: 0,
                    });
                }
            }
            BrowseCardKind::Artist | BrowseCardKind::Album | BrowseCardKind::Playlist => {
                if let Some(bid) = &card.browse_id {
                    env.actions.push(PlayAction::OpenBrowse(bid.clone()));
                }
            }
        }
    }

    if let Some(target) = ContextTarget::from_browse_card(card) {
        attach_context_menu(
            &resp,
            target,
            ContextMenuEnv {
                lang,
                playlists: env.playlists,
                sync_queue: env.sync_queue,
                toast: env.toast,
                actions: env.actions,
            },
            None,
        );
    }
}

pub(crate) fn paint_art_box(
    ui: &mut egui::Ui,
    url: &str,
    size: egui::Vec2,
    artwork: &mut ArtworkCache,
) {
    if let Some(tex) = artwork.texture_for(ui.ctx(), url) {
        ui.add(egui::Image::new((tex.id(), size)).corner_radius(4.0));
    } else {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 4.0, egui::Color32::from_gray(35));
    }
}
