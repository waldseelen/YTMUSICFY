use eframe::egui;
use ytm_core::{BrowseCardKind, BrowsePage, BrowsePageKind, Track};

use crate::artwork::ArtworkCache;
use crate::playback_ops::PlayAction;
use crate::state::AsyncState;
use crate::views::async_banner;
use crate::views::inspect_helpers::format_duration;

/// Render Home (`FEmusic_home`), Artist, Album, or Browse shelf view.
pub fn show(
    ui: &mut egui::Ui,
    browse_target: &mut String,
    browse_state: &AsyncState<BrowsePage>,
    artwork: &mut ArtworkCache,
) -> Vec<PlayAction> {
    let mut actions = Vec::new();

    ui.horizontal_wrapped(|ui| {
        if ui.button("🏠 Home (FEmusic_home)").clicked() {
            actions.push(PlayAction::OpenBrowse("FEmusic_home".to_string()));
        }
        ui.label("Browse ID:");
        let resp = ui.add(
            egui::TextEdit::singleline(browse_target)
                .desired_width(200.0)
                .hint_text("FEmusic_home / UC... / MPREb_..."),
        );
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("Go").clicked() || enter) && !browse_target.trim().is_empty() {
            actions.push(PlayAction::OpenBrowse(browse_target.trim().to_string()));
        }
        if let AsyncState::Ready(page) = browse_state
            && ui.button("↻ Refresh").clicked()
        {
            actions.push(PlayAction::OpenBrowse(page.browse_id.clone()));
        }
    });
    ui.separator();

    match browse_state {
        AsyncState::Idle => {
            ui.label("Browse YouTube Music Home recommendations, Artist profiles, or Album pages.");
            if ui.button("Load Home Recommendations").clicked() {
                actions.push(PlayAction::OpenBrowse("FEmusic_home".to_string()));
            }
            return actions;
        }
        AsyncState::Loading(msg) => {
            async_banner(ui, Some(msg.as_str()), None);
            return actions;
        }
        AsyncState::Failed(err) => {
            async_banner(ui, None, Some(err.as_str()));
            if ui.button("Retry").clicked() && !browse_target.trim().is_empty() {
                actions.push(PlayAction::OpenBrowse(browse_target.trim().to_string()));
            }
            return actions;
        }
        AsyncState::Ready(_) => {}
    }

    let Some(page) = browse_state.data() else {
        return actions;
    };

    egui::ScrollArea::vertical().show(ui, |ui| {
        render_header(ui, page, artwork, &mut actions);
        ui.add_space(8.0);

        for section in &page.sections {
            ui.group(|ui| {
                ui.heading(&section.title);
                if !section.cards.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for (idx, card) in section.cards.iter().enumerate() {
                            render_card(ui, idx, card, artwork, &mut actions);
                        }
                    });
                }
                if !section.tracks.is_empty() {
                    for (tidx, track) in section.tracks.iter().enumerate() {
                        render_track_row(ui, track, &section.tracks, tidx, artwork, &mut actions);
                    }
                }
            });
            ui.add_space(6.0);
        }
    });

    actions
}

fn render_header(
    ui: &mut egui::Ui,
    page: &BrowsePage,
    artwork: &mut ArtworkCache,
    actions: &mut Vec<PlayAction>,
) {
    ui.horizontal(|ui| {
        if let Some(url) = &page.thumbnail_url {
            paint_art_box(ui, url, egui::vec2(84.0, 84.0), artwork);
        }
        ui.vertical(|ui| {
            ui.heading(&page.title);
            let kind_str = match page.kind {
                BrowsePageKind::Home => "Home",
                BrowsePageKind::Artist => "Artist",
                BrowsePageKind::Album => "Album",
                BrowsePageKind::Playlist => "Playlist",
                BrowsePageKind::Unknown => "Browse",
            };
            ui.small(format!("{kind_str} • {}", page.browse_id));
            if let Some(sub) = &page.subtitle {
                ui.label(sub);
            }
            if !page.tracks.is_empty() {
                ui.horizontal(|ui| {
                    if ui.button("▶ Play All").clicked() {
                        actions.push(PlayAction::PlayFromList {
                            tracks: page.tracks.clone(),
                            start_idx: 0,
                        });
                    }
                    if ui.button("+ Queue All").clicked() {
                        for t in &page.tracks {
                            actions.push(PlayAction::AppendQueue(t.clone()));
                        }
                    }
                });
            }
        });
    });
}

fn render_card(
    ui: &mut egui::Ui,
    _idx: usize,
    card: &ytm_core::BrowseCard,
    artwork: &mut ArtworkCache,
    actions: &mut Vec<PlayAction>,
) {
    ui.group(|ui| {
        ui.set_width(142.0);
        ui.vertical(|ui| {
            if let Some(url) = &card.thumbnail_url {
                paint_art_box(ui, url, egui::vec2(124.0, 124.0), artwork);
            } else {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(124.0, 124.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 4.0, egui::Color32::from_gray(35));
            }
            ui.add(egui::Label::new(egui::RichText::new(&card.title).strong()).truncate());
            ui.add(egui::Label::new(egui::RichText::new(&card.subtitle).small()).truncate());
            ui.horizontal(|ui| match card.kind {
                BrowseCardKind::Track => {
                    if let Some(vid) = &card.video_id {
                        let track = Track {
                            video_id: vid.clone(),
                            set_video_id: None,
                            position: 0,
                            title: card.title.clone(),
                            artists: vec![ytm_core::ArtistRef {
                                name: card.subtitle.clone(),
                                browse_id: None,
                            }],
                            album: None,
                            duration_sec: None,
                            release_year: None,
                            music_video_type: ytm_core::MusicVideoType::AudioTrack,
                            availability: ytm_core::TrackAvailability::Available,
                            thumbnail_url: card.thumbnail_url.clone(),
                        };
                        if ui.small_button("▶").on_hover_text("Play track").clicked() {
                            actions.push(PlayAction::PlayFromList {
                                tracks: vec![track.clone()],
                                start_idx: 0,
                            });
                        }
                        if ui.small_button("+ Queue").clicked() {
                            actions.push(PlayAction::AppendQueue(track));
                        }
                    }
                }
                BrowseCardKind::Artist | BrowseCardKind::Album | BrowseCardKind::Playlist => {
                    if let Some(bid) = &card.browse_id
                        && ui.small_button("Open →").clicked()
                    {
                        actions.push(PlayAction::OpenBrowse(bid.clone()));
                    }
                }
            });
        });
    });
}

fn render_track_row(
    ui: &mut egui::Ui,
    track: &Track,
    list: &[Track],
    idx: usize,
    artwork: &mut ArtworkCache,
    actions: &mut Vec<PlayAction>,
) {
    ui.horizontal(|ui| {
        if ui.small_button("▶").clicked() {
            actions.push(PlayAction::PlayFromList {
                tracks: list.to_vec(),
                start_idx: idx,
            });
        }
        if let Some(url) = track.effective_thumbnail_url() {
            paint_art_box(ui, &url, egui::vec2(22.0, 22.0), artwork);
        }
        ui.strong(&track.title);
        if let Some(artist) = track.artists.first() {
            ui.small("—");
            if let Some(bid) = &artist.browse_id {
                if ui.link(&artist.name).clicked() {
                    actions.push(PlayAction::OpenBrowse(bid.clone()));
                }
            } else {
                ui.small(&artist.name);
            }
        }
        if let Some(album) = &track.album {
            ui.small("•");
            if let Some(bid) = &album.browse_id {
                if ui.link(&album.name).clicked() {
                    actions.push(PlayAction::OpenBrowse(bid.clone()));
                }
            } else {
                ui.small(&album.name);
            }
        }
        if let Some(dur) = track.duration_sec {
            ui.small(format_duration(u64::from(dur)));
        }
        if ui.small_button("⏭ Next").clicked() {
            actions.push(PlayAction::PlayNext(track.clone()));
        }
        if ui
            .small_button("+Q")
            .on_hover_text("Add to queue")
            .clicked()
        {
            actions.push(PlayAction::AppendQueue(track.clone()));
        }
        if ui.small_button("📻").on_hover_text("Start Radio").clicked() {
            actions.push(PlayAction::StartRadio(track.clone()));
        }
    });
}

fn paint_art_box(ui: &mut egui::Ui, url: &str, size: egui::Vec2, artwork: &mut ArtworkCache) {
    if let Some(tex) = artwork.texture_for(ui.ctx(), url) {
        ui.add(egui::Image::new((tex.id(), size)).corner_radius(4.0));
    } else {
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, 4.0, egui::Color32::from_gray(35));
    }
}
