use serde::{Deserialize, Serialize};

/// Reference to a YouTube Music artist or uploader channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ArtistRef {
    pub name: String,
    pub browse_id: Option<String>,
}

/// Reference to a YouTube Music album release.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AlbumRef {
    pub name: String,
    pub browse_id: Option<String>,
}

/// Classification of a YouTube Music item returned in `watchEndpointMusicConfig`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MusicVideoType {
    /// Official album/audio track (`MUSIC_VIDEO_TYPE_ATV`).
    AudioTrack,
    /// Official music video (`MUSIC_VIDEO_TYPE_OMV`).
    OfficialMusicVideo,
    /// User-generated content upload (`MUSIC_VIDEO_TYPE_UGC`).
    UserGenerated,
    /// Official source music (`MUSIC_VIDEO_TYPE_OFFICIAL_SOURCE_MUSIC`).
    OfficialSourceMusic,
    #[default]
    Unknown,
}

impl MusicVideoType {
    /// Parse an InnerTube `musicVideoType` enum string.
    #[must_use]
    pub fn from_innertube_str(raw: &str) -> Self {
        match raw.trim() {
            "MUSIC_VIDEO_TYPE_ATV" => Self::AudioTrack,
            "MUSIC_VIDEO_TYPE_OMV" => Self::OfficialMusicVideo,
            "MUSIC_VIDEO_TYPE_UGC" => Self::UserGenerated,
            "MUSIC_VIDEO_TYPE_OFFICIAL_SOURCE_MUSIC" => Self::OfficialSourceMusic,
            _ => Self::Unknown,
        }
    }

    /// Canonical short label for CLI tables and SQLite persistence.
    #[must_use]
    pub fn short_label(self) -> &'static str {
        match self {
            Self::AudioTrack => "ATV",
            Self::OfficialMusicVideo => "OMV",
            Self::UserGenerated => "UGC",
            Self::OfficialSourceMusic => "OSM",
            Self::Unknown => "UNK",
        }
    }
}

/// Playability state of a track inside a playlist.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum TrackAvailability {
    #[default]
    Available,
    Deleted,
    Private,
    RegionBlocked,
    Unplayable(String),
}

impl TrackAvailability {
    /// Returns true if the track is playable.
    #[must_use]
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }
}

/// Canonical domain model for a playlist or search track item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub video_id: String,
    pub set_video_id: Option<String>,
    pub position: usize,
    pub title: String,
    pub artists: Vec<ArtistRef>,
    pub album: Option<AlbumRef>,
    pub duration_sec: Option<u32>,
    pub release_year: Option<u16>,
    pub music_video_type: MusicVideoType,
    pub availability: TrackAvailability,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
}

impl Track {
    /// Primary artist display name if present.
    #[must_use]
    pub fn primary_artist_name(&self) -> Option<&str> {
        self.artists.first().map(|a| a.name.as_str())
    }

    /// Best thumbnail URL for UI display (parsed renderer URL or YouTube CDN fallback).
    #[must_use]
    pub fn effective_thumbnail_url(&self) -> Option<String> {
        if let Some(url) = &self.thumbnail_url
            && !url.trim().is_empty()
        {
            return Some(url.clone());
        }
        (!self.video_id.is_empty())
            .then(|| format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", self.video_id))
    }
}

/// Playlist visibility state in YouTube Music.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlaylistPrivacy {
    Public,
    #[default]
    Private,
    Unlisted,
}

/// Lightweight playlist entry returned by library listing (`FEmusic_liked_playlists`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistSummary {
    pub id: String,
    pub title: String,
    pub track_count: Option<usize>,
    pub owned: bool,
    pub is_liked_music: bool,
}

/// Full hydrated playlist including ordered tracks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub privacy: PlaylistPrivacy,
    pub owned: bool,
    pub track_count: usize,
    pub tracks: Vec<Track>,
}

/// Ordered move primitive emitted by the LIS minimal-move planner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveAction {
    #[serde(rename = "setVideoId")]
    pub set_video_id: String,
    /// Target successor `setVideoId`; `None` moves the item to the end of the playlist.
    #[serde(
        rename = "movedSetVideoIdSuccessor",
        skip_serializing_if = "Option::is_none"
    )]
    pub moved_set_video_successor: Option<String>,
}

/// InnerTube `browse/edit_playlist` mutation actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum MutationAction {
    #[serde(rename = "ACTION_ADD_VIDEO")]
    AddVideo {
        #[serde(rename = "addedVideoId")]
        added_video_id: Option<String>,
        #[serde(rename = "dedupeOption", skip_serializing_if = "Option::is_none")]
        dedupe_option: Option<String>,
    },
    #[serde(rename = "ACTION_REMOVE_VIDEO")]
    RemoveVideo {
        #[serde(rename = "setVideoId")]
        set_video_id: String,
        #[serde(rename = "removedVideoId")]
        removed_video_id: String,
    },
    #[serde(rename = "ACTION_MOVE_VIDEO_BEFORE")]
    MoveVideoBefore {
        #[serde(rename = "setVideoId")]
        set_video_id: String,
        #[serde(
            rename = "movedSetVideoIdSuccessor",
            skip_serializing_if = "Option::is_none"
        )]
        moved_set_video_successor: Option<String>,
    },
    #[serde(rename = "ACTION_SET_PLAYLIST_NAME")]
    SetPlaylistName {
        #[serde(rename = "playlistName")]
        playlist_name: String,
    },
    #[serde(rename = "ACTION_SET_PLAYLIST_DESCRIPTION")]
    SetPlaylistDescription {
        #[serde(rename = "playlistDescription")]
        playlist_description: String,
    },
    #[serde(rename = "ACTION_SET_PLAYLIST_PRIVACY")]
    SetPlaylistPrivacy {
        #[serde(rename = "playlistPrivacy")]
        playlist_privacy: PlaylistPrivacy,
    },
}

impl From<MoveAction> for MutationAction {
    fn from(m: MoveAction) -> Self {
        Self::MoveVideoBefore {
            set_video_id: m.set_video_id,
            moved_set_video_successor: m.moved_set_video_successor,
        }
    }
}

/// Newly assigned playlist item pair returned in `playlistEditResults`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddedPlaylistItemResult {
    pub video_id: String,
    pub set_video_id: String,
}

/// Result returned from `browse/edit_playlist`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditPlaylistResult {
    pub status: String,
    pub added_items: Vec<AddedPlaylistItemResult>,
}

/// Persisted browser credentials for InnerTube `SAPISIDHASH` authentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthCredentials {
    pub cookie: String,
    pub user_agent: String,
    pub x_goog_authuser: Option<String>,
    pub x_goog_page_id: Option<String>,
    pub x_goog_visitor_id: Option<String>,
}

/// Result of `verify_session()` canary check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub authenticated: bool,
    pub account_name: Option<String>,
    pub channel_handle: Option<String>,
    pub auth_user: String,
    pub brand_page_id: Option<String>,
}

/// Configuration parameters controlling InnerTube context and rate-limit pacing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientConfig {
    pub client_version: String,
    pub hl: String,
    pub gl: String,
    pub max_pagination_pages: usize,
    pub mutation_chunk_size: usize,
    pub mutation_delay_ms: u64,
    pub max_retries: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            client_version: "1.20261115.01.00".to_string(),
            hl: "en".to_string(),
            gl: "US".to_string(),
            max_pagination_pages: 500,
            mutation_chunk_size: 50,
            mutation_delay_ms: 600,
            max_retries: 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_action_serializes_exact_innertube_wire_shapes() {
        let add = MutationAction::AddVideo {
            added_video_id: Some("vid123".to_string()),
            dedupe_option: Some("DEDUPE_OPTION_SKIP".to_string()),
        };
        let add_json = serde_json::to_value(&add).expect("serialize add");
        assert_eq!(add_json["action"], "ACTION_ADD_VIDEO");
        assert_eq!(add_json["addedVideoId"], "vid123");
        assert_eq!(add_json["dedupeOption"], "DEDUPE_OPTION_SKIP");

        let remove = MutationAction::RemoveVideo {
            set_video_id: "SET_1".to_string(),
            removed_video_id: "vid123".to_string(),
        };
        let rem_json = serde_json::to_value(&remove).expect("serialize remove");
        assert_eq!(rem_json["action"], "ACTION_REMOVE_VIDEO");
        assert_eq!(rem_json["setVideoId"], "SET_1");
        assert_eq!(rem_json["removedVideoId"], "vid123");

        let move_end = MutationAction::from(MoveAction {
            set_video_id: "SET_1".to_string(),
            moved_set_video_successor: None,
        });
        let move_end_json = serde_json::to_value(&move_end).expect("serialize move_end");
        assert_eq!(move_end_json["action"], "ACTION_MOVE_VIDEO_BEFORE");
        assert_eq!(move_end_json["setVideoId"], "SET_1");
        assert!(move_end_json.get("movedSetVideoIdSuccessor").is_none());

        let move_before = MutationAction::from(MoveAction {
            set_video_id: "SET_1".to_string(),
            moved_set_video_successor: Some("SET_2".to_string()),
        });
        let move_before_json = serde_json::to_value(&move_before).expect("serialize move_before");
        assert_eq!(move_before_json["movedSetVideoIdSuccessor"], "SET_2");
    }

    #[test]
    fn music_video_type_parses_innertube_enums() {
        assert_eq!(
            MusicVideoType::from_innertube_str("MUSIC_VIDEO_TYPE_ATV"),
            MusicVideoType::AudioTrack
        );
        assert_eq!(
            MusicVideoType::from_innertube_str("MUSIC_VIDEO_TYPE_UGC"),
            MusicVideoType::UserGenerated
        );
        assert_eq!(
            MusicVideoType::from_innertube_str("UNRECOGNIZED"),
            MusicVideoType::Unknown
        );
    }
}
