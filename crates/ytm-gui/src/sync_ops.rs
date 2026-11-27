use serde::{Deserialize, Serialize};
use ytm_core::{MoveAction, PlaylistPrivacy, Track};

fn default_true() -> bool {
    true
}

/// Closed set of all pending remote mutations queued locally until explicit `Sync Now`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StagedOp {
    Reorder {
        playlist_id: String,
        playlist_title: String,
        target_set_video_ids: Vec<String>,
        moves: Vec<MoveAction>,
    },
    AddTracks {
        playlist_id: String,
        playlist_title: String,
        tracks: Vec<Track>,
        allow_duplicates: bool,
    },
    RemoveTracks {
        playlist_id: String,
        playlist_title: String,
        tracks: Vec<Track>,
    },
    MoveTracks {
        source_playlist_id: String,
        source_playlist_title: String,
        target_playlist_id: String,
        target_playlist_title: String,
        tracks: Vec<Track>,
    },
    CreatePlaylist {
        title: String,
        description: String,
        privacy: PlaylistPrivacy,
        tracks: Vec<Track>,
    },
    UpdatePlaylistMeta {
        playlist_id: String,
        playlist_title: String,
        title: Option<String>,
        description: Option<String>,
        privacy: Option<PlaylistPrivacy>,
    },
    DeletePlaylist {
        playlist_id: String,
        playlist_title: String,
    },
    UpgradeTracks {
        playlist_id: String,
        playlist_title: String,
        upgrades: Vec<(Track, Track)>,
        #[serde(default)]
        preserve_position: bool,
    },
    RestoreSnapshot {
        snapshot_id: String,
        playlist_id: String,
        playlist_title: String,
    },
    SetLikeStatus {
        video_id: String,
        title: String,
        liked: bool,
    },
    SetLikeStatusBatch {
        video_ids: Vec<String>,
        titles: Vec<String>,
        liked: bool,
    },
}

impl StagedOp {
    #[must_use]
    pub fn target_playlist_id(&self) -> Option<&str> {
        match self {
            Self::Reorder { playlist_id, .. }
            | Self::AddTracks { playlist_id, .. }
            | Self::RemoveTracks { playlist_id, .. }
            | Self::UpgradeTracks { playlist_id, .. }
            | Self::UpdatePlaylistMeta { playlist_id, .. }
            | Self::DeletePlaylist { playlist_id, .. }
            | Self::RestoreSnapshot { playlist_id, .. } => Some(playlist_id.as_str()),
            Self::MoveTracks {
                target_playlist_id, ..
            } => Some(target_playlist_id.as_str()),
            Self::CreatePlaylist { .. }
            | Self::SetLikeStatus { .. }
            | Self::SetLikeStatusBatch { .. } => None,
        }
    }

    /// Returns true if this operation structurally mutates the track list of `playlist_id`.
    #[must_use]
    pub fn is_structural_for(&self, playlist_id: &str) -> bool {
        match self {
            Self::Reorder {
                playlist_id: pid, ..
            }
            | Self::AddTracks {
                playlist_id: pid, ..
            }
            | Self::RemoveTracks {
                playlist_id: pid, ..
            }
            | Self::UpgradeTracks {
                playlist_id: pid, ..
            }
            | Self::DeletePlaylist {
                playlist_id: pid, ..
            }
            | Self::RestoreSnapshot {
                playlist_id: pid, ..
            } => pid == playlist_id,
            Self::MoveTracks {
                source_playlist_id,
                target_playlist_id,
                ..
            } => source_playlist_id == playlist_id || target_playlist_id == playlist_id,
            Self::CreatePlaylist { .. }
            | Self::UpdatePlaylistMeta { .. }
            | Self::SetLikeStatus { .. }
            | Self::SetLikeStatusBatch { .. } => false,
        }
    }

    #[must_use]
    pub fn kind_label(&self) -> &'static str {
        match self {
            Self::Reorder { .. } => "Reorder (LIS)",
            Self::AddTracks { .. } => "Add Tracks",
            Self::RemoveTracks { .. } => "Remove Tracks",
            Self::MoveTracks { .. } => "Move Tracks",
            Self::CreatePlaylist { .. } => "Create Playlist",
            Self::UpdatePlaylistMeta { .. } => "Edit Metadata",
            Self::DeletePlaylist { .. } => "Delete Playlist",
            Self::UpgradeTracks { .. } => "ATV Upgrade",
            Self::RestoreSnapshot { .. } => "Restore Snapshot",
            Self::SetLikeStatus { liked, .. } | Self::SetLikeStatusBatch { liked, .. } => {
                if *liked { "Like Track" } else { "Unlike Track" }
            }
        }
    }

    #[must_use]
    pub fn target_label(&self) -> &str {
        match self {
            Self::Reorder { playlist_title, .. }
            | Self::AddTracks { playlist_title, .. }
            | Self::RemoveTracks { playlist_title, .. }
            | Self::UpgradeTracks { playlist_title, .. }
            | Self::UpdatePlaylistMeta { playlist_title, .. }
            | Self::DeletePlaylist { playlist_title, .. }
            | Self::RestoreSnapshot { playlist_title, .. } => playlist_title.as_str(),
            Self::MoveTracks {
                target_playlist_title,
                ..
            } => target_playlist_title.as_str(),
            Self::CreatePlaylist { title, .. } => title.as_str(),
            Self::SetLikeStatus { title, .. } => title.as_str(),
            Self::SetLikeStatusBatch { .. } => "Liked Music",
        }
    }

    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Reorder {
                playlist_title,
                moves,
                ..
            } => format!("Reorder '{playlist_title}' with {} LIS moves", moves.len()),
            Self::AddTracks {
                playlist_title,
                tracks,
                ..
            } => format!("Add {} tracks to '{playlist_title}'", tracks.len()),
            Self::RemoveTracks {
                playlist_title,
                tracks,
                ..
            } => format!("Remove {} tracks from '{playlist_title}'", tracks.len()),
            Self::MoveTracks {
                source_playlist_title,
                target_playlist_title,
                tracks,
                ..
            } => format!(
                "Move {} tracks '{source_playlist_title}' -> '{target_playlist_title}'",
                tracks.len()
            ),
            Self::CreatePlaylist { title, tracks, .. } => {
                format!("Create playlist '{title}' with {} tracks", tracks.len())
            }
            Self::UpdatePlaylistMeta {
                playlist_title,
                title,
                description,
                privacy,
                ..
            } => {
                let mut fields = Vec::new();
                if title.is_some() {
                    fields.push("title");
                }
                if description.is_some() {
                    fields.push("description");
                }
                if privacy.is_some() {
                    fields.push("privacy");
                }
                let joined = if fields.is_empty() {
                    "none".to_string()
                } else {
                    fields.join(", ")
                };
                format!("Update metadata ({joined}) on '{playlist_title}'")
            }
            Self::DeletePlaylist { playlist_title, .. } => {
                format!("Delete playlist '{playlist_title}'")
            }
            Self::UpgradeTracks {
                playlist_title,
                upgrades,
                ..
            } => format!(
                "Upgrade {} tracks to Official ATV in '{playlist_title}'",
                upgrades.len()
            ),
            Self::RestoreSnapshot {
                snapshot_id,
                playlist_title,
                ..
            } => format!("Restore snapshot '{snapshot_id}' onto '{playlist_title}'"),
            Self::SetLikeStatus { title, liked, .. } => {
                let verb = if *liked { "Like" } else { "Unlike" };
                format!("{verb} '{title}'")
            }
            Self::SetLikeStatusBatch {
                video_ids, liked, ..
            } => {
                let verb = if *liked { "Like" } else { "Unlike" };
                format!("{} {} tracks in Liked Music", verb, video_ids.len())
            }
        }
    }
}

/// Sequenced entry in the local Sync staging queue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedEntry {
    pub id: u64,
    pub op: StagedOp,
    #[serde(default = "default_true")]
    pub selected: bool,
}
