pub mod auth;
pub mod browse_models;
pub mod client;
mod client_browse;
mod client_mutations;
mod client_playlist;
pub mod error;
pub mod fetch_models;
pub mod http;
pub mod models;
pub mod parser;
pub mod traits;

pub use auth::{
    build_auth_headers, compute_sapisidhash, extract_sapisid, netscape_cookies_to_header,
    parse_auth_input,
};
pub use browse_models::{
    BROWSE_EXPLORE, BROWSE_HISTORY, BROWSE_HOME, BROWSE_LIKED_ALBUMS, BROWSE_LIKED_PLAYLISTS,
    BrowseCard, BrowseCardKind, BrowsePage, BrowsePageKind, BrowseSection, LyricsPayload,
    SearchFilter, SearchPage, TimedLyricLine, WatchNextResult,
};
pub use client::InnerTubeClient;
pub use error::CoreError;
pub use fetch_models::{
    ChunkOutcome, FetchCompleteness, FetchManifest, MutationBatchReport, PlaylistFetchResult,
};
pub use http::{MockTransport, RecordedRequest, ReqwestTransport, RetryPolicy};
pub use models::{
    AddedPlaylistItemResult, AlbumRef, ArtistRef, AuthCredentials, ClientConfig,
    EditPlaylistResult, MoveAction, MusicVideoType, MutationAction, Playlist, PlaylistAccess,
    PlaylistPrivacy, PlaylistSummary, SessionStatus, Track, TrackAvailability,
};
pub use traits::{HttpTransport, InnerTubeClientTrait};
