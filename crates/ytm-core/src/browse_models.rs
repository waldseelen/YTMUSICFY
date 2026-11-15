use serde::{Deserialize, Serialize};

use crate::models::Track;

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
        if id == "FEmusic_home" {
            Self::Home
        } else if id.starts_with("UC") || id.starts_with("MPLA") {
            Self::Artist
        } else if id.starts_with("MPREb_") {
            Self::Album
        } else if id.starts_with("VL") || id.starts_with("PL") || id == "LM" {
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

/// Clickable card rendered on Home, Artist, or Album shelves.
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
}

/// Normalized Home / Artist / Album browse payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowsePage {
    pub browse_id: String,
    pub kind: BrowsePageKind,
    pub title: String,
    pub subtitle: Option<String>,
    pub thumbnail_url: Option<String>,
    pub sections: Vec<BrowseSection>,
    pub tracks: Vec<Track>,
}

/// Watch-next (`next`) radio queue and optional lyrics browse handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchNextResult {
    pub tracks: Vec<Track>,
    pub lyrics_browse_id: Option<String>,
}
