use async_trait::async_trait;
use serde_json::Value;

use crate::browse_models::{BrowsePage, LyricsPayload, SearchFilter, SearchPage, WatchNextResult};
use crate::error::CoreError;
use crate::fetch_models::{FetchManifest, MutationBatchReport, PlaylistFetchResult};
use crate::http::RetryPolicy;
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

    /// Execute a POST JSON request governed by an explicit [`RetryPolicy`].
    async fn post_json_with_policy(
        &self,
        endpoint: &str,
        body: &Value,
        headers: &[(String, String)],
        _policy: RetryPolicy,
    ) -> Result<Value, CoreError> {
        self.post_json(endpoint, body, headers).await
    }

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

    /// Rename a playlist or update its description and privacy via `browse/edit_playlist`.
    async fn update_playlist_metadata(
        &self,
        playlist_id: &str,
        title: Option<&str>,
        description: Option<&str>,
        privacy: Option<PlaylistPrivacy>,
    ) -> Result<(), CoreError>;

    /// Like (`liked = true`) or remove like (`liked = false`) for a track video ID.
    async fn set_like_status(&self, video_id: &str, liked: bool) -> Result<(), CoreError>;

    /// Fetch Home (`FEmusic_home`), Artist (`UC...`/`MPLA...`), Album (`MPREb_...`), or Playlist browse page.
    async fn get_browse_page(&self, browse_id: &str) -> Result<BrowsePage, CoreError>;

    /// Fetch a browse page with optional InnerTube `params` (e.g. artist "See all" subpages).
    async fn get_browse_page_with_params(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<BrowsePage, CoreError> {
        let _ = params;
        self.get_browse_page(browse_id).await
    }

    /// Fetch watch-next radio queue and optional lyrics browse ID for a seed track/playlist.
    async fn get_watch_next(
        &self,
        video_id: &str,
        playlist_id: Option<&str>,
    ) -> Result<WatchNextResult, CoreError>;

    /// Fetch plain-text song lyrics for an `MPLYt_...` browse ID.
    async fn get_lyrics(&self, lyrics_browse_id: &str) -> Result<Option<String>, CoreError>;

    /// Fetch a playlist together with its [`FetchManifest`] completeness telemetry.
    async fn get_playlist_with_manifest(
        &self,
        playlist_id: &str,
    ) -> Result<PlaylistFetchResult, CoreError> {
        let playlist = self.get_playlist(playlist_id).await?;
        let manifest = FetchManifest::from_playlist_complete(&playlist);
        Ok(PlaylistFetchResult { playlist, manifest })
    }

    /// Fetch user's Liked Music (`"VLLM"`) together with its [`FetchManifest`].
    async fn get_liked_songs_with_manifest(&self) -> Result<PlaylistFetchResult, CoreError> {
        self.get_playlist_with_manifest("LM").await
    }

    /// Add `video_ids` in chunked batches and return per-chunk outcomes.
    async fn add_playlist_items_report(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<MutationBatchReport, CoreError> {
        let res = self
            .add_playlist_items(playlist_id, video_ids, allow_duplicates)
            .await?;
        Ok(MutationBatchReport::from_single_result(
            video_ids.len(),
            res,
        ))
    }

    /// Remove playlist entries and return per-chunk outcomes.
    async fn remove_playlist_items_report(
        &self,
        playlist_id: &str,
        items: &[(String, String)],
    ) -> Result<MutationBatchReport, CoreError> {
        let res = self.remove_playlist_items(playlist_id, items).await?;
        Ok(MutationBatchReport::from_single_result(items.len(), res))
    }

    /// Move playlist entries and return per-chunk outcomes.
    async fn move_playlist_items_report(
        &self,
        playlist_id: &str,
        moves: &[MoveAction],
    ) -> Result<MutationBatchReport, CoreError> {
        let results = self.move_playlist_items(playlist_id, moves).await?;
        Ok(MutationBatchReport::from_chunk_results(
            moves.len(),
            results,
        ))
    }

    /// Search YouTube Music catalog by [`SearchFilter`] category with optional continuation token.
    async fn search_catalog(
        &self,
        query: &str,
        _filter: SearchFilter,
        continuation: Option<&str>,
    ) -> Result<SearchPage, CoreError> {
        if continuation.is_some() {
            return Ok(SearchPage {
                tracks: Vec::new(),
                cards: Vec::new(),
                continuation: None,
            });
        }
        let tracks = self.search_tracks(query, 25).await?;
        Ok(SearchPage {
            tracks,
            cards: Vec::new(),
            continuation: None,
        })
    }

    /// Fetch the next page of a paginated browse view (`FEmusic_explore`, `FEmusic_history`, etc.).
    async fn get_browse_continuation(&self, token: &str) -> Result<BrowsePage, CoreError> {
        self.get_browse_page(token).await
    }

    /// Fetch time-synced or plain-text lyrics for an `MPLYt_...` browse ID.
    async fn get_lyrics_payload(
        &self,
        lyrics_browse_id: &str,
    ) -> Result<Option<LyricsPayload>, CoreError> {
        Ok(self
            .get_lyrics(lyrics_browse_id)
            .await?
            .map(LyricsPayload::Plain))
    }

    /// Fetch autocomplete search suggestions (`POST music/get_search_suggestions`).
    async fn get_search_suggestions(&self, _query: &str) -> Result<Vec<String>, CoreError> {
        Ok(Vec::new())
    }
}
