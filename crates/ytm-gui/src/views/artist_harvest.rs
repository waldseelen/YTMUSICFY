use std::collections::HashMap;

use eframe::egui;
use ytm_core::{ArtistHarvest, Playlist, PlaylistSummary, Track};
use ytm_ops::{ArtistHarvestConfig, plan_artist_harvest};

use crate::navigation::ToastState;
use crate::sync_queue::{StagedOp, SyncQueue};

/// Destination chosen for an "All of Artist" harvest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtistHarvestTarget {
    NewPlaylist {
        title: String,
    },
    ExistingPlaylist {
        playlist_id: String,
        playlist_title: String,
    },
}

/// Editable preview state for a completed artist harvest.
pub struct ArtistHarvestModal {
    pub harvest: ArtistHarvest,
    pub include_features: bool,
    pub include_compilations: bool,
    pub target: ArtistHarvestTarget,
    pub new_title: String,
    pub existing_id: Option<String>,
    pub error: Option<String>,
}

impl ArtistHarvestModal {
    #[must_use]
    pub fn new(harvest: ArtistHarvest) -> Self {
        let new_title = format!("{} — All Songs", harvest.artist_name);
        Self {
            harvest,
            include_features: true,
            include_compilations: true,
            target: ArtistHarvestTarget::NewPlaylist {
                title: new_title.clone(),
            },
            new_title,
            existing_id: None,
            error: None,
        }
    }

    fn config(&self) -> ArtistHarvestConfig {
        ArtistHarvestConfig {
            include_features: self.include_features,
            include_compilations: self.include_compilations,
            skip_existing: true,
        }
    }

    fn resolved_target(&self) -> ArtistHarvestTarget {
        match &self.target {
            ArtistHarvestTarget::NewPlaylist { .. } => ArtistHarvestTarget::NewPlaylist {
                title: self.new_title.trim().to_string(),
            },
            ArtistHarvestTarget::ExistingPlaylist {
                playlist_id,
                playlist_title,
            } => ArtistHarvestTarget::ExistingPlaylist {
                playlist_id: playlist_id.clone(),
                playlist_title: playlist_title.clone(),
            },
        }
    }
}

/// Deduplicate and stage a harvest into the local Sync queue without touching the network.
pub fn stage_artist_harvest(
    queue: &mut SyncQueue,
    harvest: &ArtistHarvest,
    existing: &[Track],
    config: &ArtistHarvestConfig,
    target: &ArtistHarvestTarget,
) -> Result<usize, String> {
    let plan = plan_artist_harvest(harvest, existing, config);
    if plan.tracks.is_empty() {
        return Err("No new tracks to stage after filters and duplicate removal.".to_string());
    }
    let count = plan.track_count();
    match target {
        ArtistHarvestTarget::NewPlaylist { title } => {
            if title.trim().is_empty() {
                return Err("Playlist title cannot be empty.".to_string());
            }
            queue.push(StagedOp::CreatePlaylist {
                title: title.trim().to_string(),
                description: format!("All of Artist harvest: {}", harvest.artist_name),
                privacy: ytm_core::PlaylistPrivacy::Private,
                tracks: plan.tracks,
            });
        }
        ArtistHarvestTarget::ExistingPlaylist {
            playlist_id,
            playlist_title,
        } => {
            queue.push(StagedOp::AddTracks {
                playlist_id: playlist_id.clone(),
                playlist_title: playlist_title.clone(),
                tracks: plan.tracks,
                allow_duplicates: false,
            });
        }
    }
    Ok(count)
}

/// Render the artist harvest preview dialog and stage the chosen plan.
pub fn show_artist_harvest_modal(
    ctx: &egui::Context,
    modal: &mut Option<ArtistHarvestModal>,
    playlists: &[PlaylistSummary],
    playlist_cache: &HashMap<String, Playlist>,
    queue: &mut SyncQueue,
    toast: &mut Option<ToastState>,
) {
    let Some(state) = modal.as_mut() else {
        return;
    };
    let mut close = false;
    let mut stage = false;

    egui::Window::new(format!("All of Artist: {}", state.harvest.artist_name))
        .id(egui::Id::new("artist_harvest_modal"))
        .collapsible(false)
        .resizable(true)
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "{} releases discovered across {} page(s), {} track candidates.",
                state.harvest.releases.len(),
                state.harvest.pages_fetched,
                state.harvest.tracks.len()
            ));
            ui.separator();
            ui.checkbox(&mut state.include_features, "Include featured appearances");
            ui.checkbox(
                &mut state.include_compilations,
                "Include compilations / soundtracks",
            );
            ui.separator();

            let new_selected = matches!(state.target, ArtistHarvestTarget::NewPlaylist { .. });
            if ui
                .radio(new_selected, "Create a new playlist")
                .clicked()
                && !new_selected
            {
                state.target = ArtistHarvestTarget::NewPlaylist {
                    title: state.new_title.clone(),
                };
            }
            if new_selected {
                ui.text_edit_singleline(&mut state.new_title);
            }

            let existing_selected = !new_selected;
            if ui
                .radio(existing_selected, "Add to an existing playlist")
                .clicked()
                && !existing_selected
            {
                state.target = ArtistHarvestTarget::ExistingPlaylist {
                    playlist_id: String::new(),
                    playlist_title: String::new(),
                };
            }
            if existing_selected {
                let current = state.existing_id.clone().unwrap_or_default();
                let selected_title = playlists
                    .iter()
                    .find(|p| p.id == current)
                    .map_or("<select playlist>", |p| p.title.as_str());
                egui::ComboBox::from_id_salt("artist_harvest_target_playlist")
                    .selected_text(selected_title)
                    .show_ui(ui, |ui| {
                        for playlist in playlists {
                            if ui
                                .selectable_label(current == playlist.id, &playlist.title)
                                .clicked()
                            {
                                state.existing_id = Some(playlist.id.clone());
                                state.target = ArtistHarvestTarget::ExistingPlaylist {
                                    playlist_id: playlist.id.clone(),
                                    playlist_title: playlist.title.clone(),
                                };
                            }
                        }
                    });
            }

            let existing: &[Track] = state
                .existing_id
                .as_deref()
                .and_then(|id| playlist_cache.get(id))
                .map_or(&[], |p| p.tracks.as_slice());
            let plan = plan_artist_harvest(&state.harvest, existing, &state.config());
            ui.separator();
            ui.label(format!(
                "Preview: {} track(s) will be staged ({} duplicates removed, {} already in playlist, {} filtered).",
                plan.track_count(),
                plan.duplicates_removed,
                plan.skipped_existing,
                plan.filtered_features + plan.filtered_compilations
            ));
            ui.weak(format!(
                "Songs {} · Albums {} · Singles {} · Appears on {}",
                plan.kept_from_songs,
                plan.kept_from_albums,
                plan.kept_from_singles,
                plan.kept_from_appears_on
            ));
            if let Some(err) = &state.error {
                ui.colored_label(egui::Color32::from_rgb(235, 95, 95), err);
            }
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    close = true;
                }
                let can_stage = plan.track_count() > 0
                    && match &state.target {
                        ArtistHarvestTarget::NewPlaylist { .. } => {
                            !state.new_title.trim().is_empty()
                        }
                        ArtistHarvestTarget::ExistingPlaylist { playlist_id, .. } => {
                            !playlist_id.is_empty()
                        }
                    };
                if ui
                    .add_enabled(can_stage, egui::Button::new("Stage into Sync Queue"))
                    .clicked()
                {
                    stage = true;
                }
            });
        });

    if stage {
        let target = state.resolved_target();
        let existing: &[Track] = state
            .existing_id
            .as_deref()
            .and_then(|id| playlist_cache.get(id))
            .map_or(&[], |p| p.tracks.as_slice());
        match stage_artist_harvest(queue, &state.harvest, existing, &state.config(), &target) {
            Ok(count) => {
                *toast = Some(ToastState::info(
                    true,
                    format!("Staged {count} harvested track(s). Review in the Sync drawer."),
                ));
                close = true;
            }
            Err(err) => state.error = Some(err),
        }
    }
    if close {
        *modal = None;
    }
}
