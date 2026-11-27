use serde::{Deserialize, Serialize};

use crate::models::{ArtistRef, Track};

/// Canonical InnerTube browse IDs for top-level YouTube Music feeds.
pub const BROWSE_HOME: &str = "FEmusic_home";
pub const BROWSE_EXPLORE: &str = "FEmusic_explore";
pub const BROWSE_HISTORY: &str = "FEmusic_history";
pub const BROWSE_LIKED_ALBUMS: &str = "FEmusic_liked_albums";
pub const BROWSE_LIKED_PLAYLISTS: &str = "FEmusic_liked_playlists";

/// Kind of browse destination loaded from InnerTube `browse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowsePageKind {
    Home,
    Artist,
    Album,
    Playlist,
    Unknown,
}

impl BrowsePageKind {
    #[must_use]
    pub fn from_browse_id(id: &str) -> Self {
        if id == BROWSE_HOME {
            Self::Home
        } else if id.starts_with("UC") || id.starts_with("MPLA") || id.starts_with("MPAD") {
            Self::Artist
        } else if id.starts_with("MPRE") {
            Self::Album
        } else if id.starts_with("VL")
            || id.starts_with("PL")
            || id.starts_with("OLAK")
            || id.starts_with("RD")
            || id.starts_with("UU")
            || id.starts_with("FL")
            || id.starts_with("LL")
            || id.eq_ignore_ascii_case("LM")
        {
            Self::Playlist
        } else {
            Self::Unknown
        }
    }
}

/// Entity kind represented by a browse shelf card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowseCardKind {
    Track,
    Album,
    Artist,
    Playlist,
}

/// Clickable card rendered on Home, Artist, Album, or Search shelves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowseCard {
    pub kind: BrowseCardKind,
    pub browse_id: Option<String>,
    pub video_id: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub thumbnail_url: Option<String>,
}

/// Section shelf inside a `BrowsePage` (e.g. "Top songs", "Albums", "Quick picks").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowseSection {
    pub title: String,
    pub cards: Vec<BrowseCard>,
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub browse_id: Option<String>,
    #[serde(default)]
    pub browse_params: Option<String>,
}

/// Normalized Home / Explore / History / Artist / Album browse payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowsePage {
    pub browse_id: String,
    pub kind: BrowsePageKind,
    pub title: String,
    pub subtitle: Option<String>,
    #[serde(default)]
    pub artists: Vec<ArtistRef>,
    pub thumbnail_url: Option<String>,
    pub sections: Vec<BrowseSection>,
    pub tracks: Vec<Track>,
    #[serde(default)]
    pub continuation: Option<String>,
}

/// Watch-next (`next`) radio queue and optional lyrics browse handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchNextResult {
    pub tracks: Vec<Track>,
    pub lyrics_browse_id: Option<String>,
}

/// Filtered search category supported by InnerTube `POST search`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum SearchFilter {
    #[default]
    Top,
    Songs,
    Videos,
    Albums,
    Artists,
    CommunityPlaylists,
}

impl SearchFilter {
    /// Exact InnerTube `params` filter string for `POST search` (`None` for unfiltered `Top`).
    #[must_use]
    pub fn innertube_params(self) -> Option<&'static str> {
        match self {
            Self::Top => None,
            Self::Songs => Some("EgWKAQIIAWoMEA4QChADEAQQCRAF"),
            Self::Videos => Some("EgWKAQIQAWoMEA4QChADEAQQCRAF"),
            Self::Albums => Some("EgWKAQIYAWoMEA4QChADEAQQCRAF"),
            Self::Artists => Some("EgWKAQIgAWoMEA4QChADEAQQCRAF"),
            Self::CommunityPlaylists => Some("EgeKAQQoAEABagwQDhAKEAMQBBAJEAU%3D"),
        }
    }
}

/// Paginated search results across songs, videos, albums, artists, and community playlists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchPage {
    pub tracks: Vec<Track>,
    pub cards: Vec<BrowseCard>,
    #[serde(default)]
    pub continuation: Option<String>,
}

/// Single time-synchronized lyric line (`timedLyricsData`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedLyricLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Song lyrics payload supporting both plain text and time-synced lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricsPayload {
    Plain(String),
    Synced(Vec<TimedLyricLine>),
}
