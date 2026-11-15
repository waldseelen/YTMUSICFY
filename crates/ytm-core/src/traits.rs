use async_trait::async_trait;
use serde_json::Value;

use crate::error::CoreError;
use crate::models::{
    EditPlaylistResult, MoveAction, Playlist, PlaylistPrivacy, PlaylistSummary, SessionStatus,
    Track,
};

/// Low-level HTTP transport boundary enabling deterministic mocked responses in tests.
#[async_trait]
pub trait HttpTransport: Send + Sync {
    /// Execute a POST JSON request against an InnerTube endpoint (relative to `/youtubei/v1/`).
    async fn post_json(
        &self,
        endpoint: &str,
        body: &Value,
        headers: &[(String, String)],
    ) -> Result<Value, CoreError>;

    /// Execute a GET request returning raw response text (used for `VISITOR_DATA` discovery).
    async fn get_text(&self, url: &str, headers: &[(String, String)]) -> Result<String, CoreError>;
}

/// High-level async YouTube Music InnerTube contract implemented by `ytm-core` and mock clients.
#[async_trait]
pub trait InnerTubeClientTrait: Send + Sync {
    /// Verify active authenticated session via canary check (rejects logged-out `200 OK` payloads).
    async fn verify_session(&self) -> Result<SessionStatus, CoreError>;

    /// List library playlists including Liked Music (`LM`).
    async fn get_library_playlists(&self) -> Result<Vec<PlaylistSummary>, CoreError>;

    /// Fetch a complete playlist with all continuation pages resolved (`"LM"` normalizes to `"VLLM"`).
    async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist, CoreError>;

    /// Fetch user's Liked Music (`"VLLM"`).
    async fn get_liked_songs(&self) -> Result<Playlist, CoreError>;

    /// Create a new playlist and return its canonical playlist ID.
    async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        video_ids: &[String],
    ) -> Result<String, CoreError>;

    /// Permanently delete an owned playlist.
    async fn delete_playlist(&self, playlist_id: &str) -> Result<(), CoreError>;

    /// Add `video_ids` to a playlist in chunked 50-item batches.
    async fn add_playlist_items(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<EditPlaylistResult, CoreError>;

    /// Remove specific playlist entries identified by `(video_id, set_video_id)` pairs.
    async fn remove_playlist_items(
        &self,
        playlist_id: &str,
        items: &[(String, String)],
    ) -> Result<EditPlaylistResult, CoreError>;

    /// Execute ordered minimal-move actions (`ACTION_MOVE_VIDEO_BEFORE`) sequentially.
    async fn move_playlist_items(
        &self,
        playlist_id: &str,
        moves: &[MoveAction],
    ) -> Result<Vec<EditPlaylistResult>, CoreError>;

    /// Search tracks (primarily to discover official `ATV` candidates for `UGC`/`OMV` upgrades).
    async fn search_tracks(&self, query: &str, limit: usize) -> Result<Vec<Track>, CoreError>;
}
