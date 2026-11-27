use async_trait::async_trait;
use serde_json::{Value, json};

use crate::auth::{build_auth_headers, current_unix_timestamp};
use crate::browse_models::{BrowsePage, LyricsPayload, SearchFilter, SearchPage, WatchNextResult};
use crate::error::CoreError;
use crate::fetch_models::{ChunkOutcome, MutationBatchReport, PlaylistFetchResult};
use crate::http::RetryPolicy;
use crate::models::{
    AuthCredentials, ClientConfig, EditPlaylistResult, MoveAction, MutationAction, Playlist,
    PlaylistPrivacy, PlaylistSummary, SessionStatus, Track,
};
use crate::parser::{
    check_innertube_error, normalize_playlist_id, parse_delete_playlist_response,
    parse_lyrics_response, parse_search_tracks, parse_session_status, parse_watch_next,
};
use crate::traits::{HttpTransport, InnerTubeClientTrait};

const SEARCH_SONGS_PARAMS: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";

/// Resilient YouTube Music InnerTube client backed by any [`HttpTransport`].
pub struct InnerTubeClient<T: HttpTransport> {
    pub(crate) transport: T,
    pub(crate) credentials: AuthCredentials,
    pub(crate) config: ClientConfig,
    now_fn: fn() -> u64,
}

impl<T: HttpTransport> InnerTubeClient<T> {
    /// Create a client with default wall-clock timestamp provider.
    #[must_use]
    pub fn new(transport: T, credentials: AuthCredentials, config: ClientConfig) -> Self {
        Self {
            transport,
            credentials,
            config,
            now_fn: current_unix_timestamp,
        }
    }

    /// Create a client with deterministic timestamp injection for signing tests.
    #[must_use]
    pub fn with_clock(
        transport: T,
        credentials: AuthCredentials,
        config: ClientConfig,
        now_fn: fn() -> u64,
    ) -> Self {
        Self {
            transport,
            credentials,
            config,
            now_fn,
        }
    }

    /// Access underlying transport (useful in tests inspecting recorded payloads).
    #[must_use]
    pub fn transport(&self) -> &T {
        &self.transport
    }

    fn build_context(&self) -> Value {
        let mut user = serde_json::Map::new();
        if let Some(page_id) = &self.credentials.x_goog_page_id
            && !page_id.trim().is_empty()
        {
            user.insert("onBehalfOfUser".to_string(), Value::String(page_id.clone()));
        }

        // `context.user` MUST sit as sibling of `context.client`, never nested inside `client`.
        json!({
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": self.config.client_version,
                "hl": self.config.normalized_hl(),
                "gl": self.config.normalized_gl(),
            },
            "user": Value::Object(user),
        })
    }

    pub(crate) async fn post_authed_with_policy(
        &self,
        endpoint: &str,
        mut payload: Value,
        policy: RetryPolicy,
    ) -> Result<Value, CoreError> {
        let headers = build_auth_headers(&self.credentials, (self.now_fn)())?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("context".to_string(), self.build_context());
        }
        self.transport
            .post_json_with_policy(endpoint, &payload, &headers, policy)
            .await
    }
}

#[async_trait]
impl<T: HttpTransport> InnerTubeClientTrait for InnerTubeClient<T> {
    async fn verify_session(&self) -> Result<SessionStatus, CoreError> {
        let resp = self
            .post_authed_with_policy(
                "account/account_menu",
                json!({}),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        let auth_user = self.credentials.x_goog_authuser.as_deref().unwrap_or("0");
        parse_session_status(&resp, auth_user, self.credentials.x_goog_page_id.as_deref())
    }

    async fn get_library_playlists(&self) -> Result<Vec<PlaylistSummary>, CoreError> {
        self.fetch_library_playlists_paginated().await
    }

    async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist, CoreError> {
        self.get_playlist_with_manifest(playlist_id)
            .await
            .map(|res| res.playlist)
    }

    async fn get_liked_songs(&self) -> Result<Playlist, CoreError> {
        self.get_playlist("LM").await
    }

    async fn get_playlist_with_manifest(
        &self,
        playlist_id: &str,
    ) -> Result<PlaylistFetchResult, CoreError> {
        self.fetch_playlist_with_manifest_impl(playlist_id).await
    }

    async fn get_liked_songs_with_manifest(&self) -> Result<PlaylistFetchResult, CoreError> {
        self.get_playlist_with_manifest("LM").await
    }

    async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        video_ids: &[String],
    ) -> Result<String, CoreError> {
        self.create_playlist_chunked(title, description, privacy, video_ids)
            .await
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<(), CoreError> {
        let clean_id = normalize_playlist_id(playlist_id);
        let resp = self
            .post_authed_with_policy(
                "playlist/delete",
                json!({ "playlistId": clean_id }),
                RetryPolicy::MutationSafe,
            )
            .await?;
        parse_delete_playlist_response(&resp)
    }

    async fn add_playlist_items(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<EditPlaylistResult, CoreError> {
        let actions = Self::build_add_actions(video_ids, allow_duplicates);
        let (report, err) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        if let Some(first_err) = err {
            return Err(first_err);
        }
        Ok(EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items: report.added_items(),
        })
    }

    async fn add_playlist_items_report(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<MutationBatchReport, CoreError> {
        let actions = Self::build_add_actions(video_ids, allow_duplicates);
        let (report, _) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        Ok(report)
    }

    async fn remove_playlist_items(
        &self,
        playlist_id: &str,
        items: &[(String, String)],
    ) -> Result<EditPlaylistResult, CoreError> {
        let actions = Self::build_remove_actions(items)?;
        let (_, err) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        if let Some(first_err) = err {
            return Err(first_err);
        }
        Ok(EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items: Vec::new(),
        })
    }

    async fn remove_playlist_items_report(
        &self,
        playlist_id: &str,
        items: &[(String, String)],
    ) -> Result<MutationBatchReport, CoreError> {
        let actions = Self::build_remove_actions(items)?;
        let (report, _) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        Ok(report)
    }

    async fn move_playlist_items(
        &self,
        playlist_id: &str,
        moves: &[MoveAction],
    ) -> Result<Vec<EditPlaylistResult>, CoreError> {
        let actions: Vec<MutationAction> =
            moves.iter().cloned().map(MutationAction::from).collect();
        let (report, err) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        if let Some(first_err) = err {
            return Err(first_err);
        }
        Ok(report
            .outcomes
            .into_iter()
            .filter_map(|o| match o {
                ChunkOutcome::Succeeded { result, .. } => Some(result),
                ChunkOutcome::Failed { .. } => None,
            })
            .collect())
    }

    async fn move_playlist_items_report(
        &self,
        playlist_id: &str,
        moves: &[MoveAction],
    ) -> Result<MutationBatchReport, CoreError> {
        let actions: Vec<MutationAction> =
            moves.iter().cloned().map(MutationAction::from).collect();
        let (report, _) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        Ok(report)
    }

    async fn search_tracks(&self, query: &str, limit: usize) -> Result<Vec<Track>, CoreError> {
        let resp = self
            .post_authed_with_policy(
                "search",
                json!({
                    "query": query,
                    "params": SEARCH_SONGS_PARAMS,
                }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        check_innertube_error(&resp)?;
        Ok(parse_search_tracks(&resp, limit))
    }

    async fn update_playlist_metadata(
        &self,
        playlist_id: &str,
        title: Option<&str>,
        description: Option<&str>,
        privacy: Option<PlaylistPrivacy>,
    ) -> Result<(), CoreError> {
        self.update_playlist_metadata_impl(playlist_id, title, description, privacy)
            .await
    }

    async fn set_like_status(&self, video_id: &str, liked: bool) -> Result<(), CoreError> {
        let endpoint = if liked {
            "like/like"
        } else {
            "like/removelike"
        };
        let resp = self
            .post_authed_with_policy(
                endpoint,
                json!({ "target": { "videoId": video_id } }),
                RetryPolicy::MutationSafe,
            )
            .await?;
        check_innertube_error(&resp)?;
        Ok(())
    }

    async fn get_browse_page(&self, browse_id: &str) -> Result<BrowsePage, CoreError> {
        self.get_browse_page_impl(browse_id, None).await
    }

    async fn get_browse_page_with_params(
        &self,
        browse_id: &str,
        params: Option<&str>,
    ) -> Result<BrowsePage, CoreError> {
        self.get_browse_page_impl(browse_id, params).await
    }

    async fn get_watch_next(
        &self,
        video_id: &str,
        playlist_id: Option<&str>,
    ) -> Result<WatchNextResult, CoreError> {
        let mut body = json!({
            "videoId": video_id,
            "isAudioOnly": true,
            "enablePersistentPlaylistPanel": true,
        });
        if let Some(pid) = playlist_id
            && !pid.trim().is_empty()
        {
            body.as_object_mut()
                .expect("object")
                .insert("playlistId".to_string(), json!(normalize_playlist_id(pid)));
        }
        let resp = self
            .post_authed_with_policy("next", body, RetryPolicy::IdempotentRead)
            .await?;
        parse_watch_next(&resp)
    }

    async fn get_lyrics(&self, lyrics_browse_id: &str) -> Result<Option<String>, CoreError> {
        let resp = self
            .post_authed_with_policy(
                "browse",
                json!({ "browseId": lyrics_browse_id }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        check_innertube_error(&resp)?;
        Ok(parse_lyrics_response(&resp))
    }

    async fn search_catalog(
        &self,
        query: &str,
        filter: SearchFilter,
        continuation: Option<&str>,
    ) -> Result<SearchPage, CoreError> {
        self.search_catalog_impl(query, filter, continuation).await
    }

    async fn get_browse_continuation(&self, token: &str) -> Result<BrowsePage, CoreError> {
        self.get_browse_continuation_impl(token).await
    }

    async fn get_lyrics_payload(
        &self,
        lyrics_browse_id: &str,
    ) -> Result<Option<LyricsPayload>, CoreError> {
        self.get_lyrics_payload_impl(lyrics_browse_id).await
    }

    async fn get_search_suggestions(&self, query: &str) -> Result<Vec<String>, CoreError> {
        self.get_search_suggestions_impl(query).await
    }
}
