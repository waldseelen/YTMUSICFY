pub mod auth;
pub mod client;
pub mod error;
pub mod http;
pub mod models;
pub mod parser;
pub mod traits;

pub use auth::{build_auth_headers, compute_sapisidhash, extract_sapisid, parse_auth_input};
pub use client::InnerTubeClient;
pub use error::CoreError;
pub use http::{MockTransport, RecordedRequest, ReqwestTransport};
pub use models::{
    AddedPlaylistItemResult, AlbumRef, ArtistRef, AuthCredentials, ClientConfig,
    EditPlaylistResult, MoveAction, MusicVideoType, MutationAction, Playlist, PlaylistPrivacy,
    PlaylistSummary, SessionStatus, Track, TrackAvailability,
};
pub use traits::{HttpTransport, InnerTubeClientTrait};
