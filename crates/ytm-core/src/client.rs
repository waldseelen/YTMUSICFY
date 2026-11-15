use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::auth::{build_auth_headers, current_unix_timestamp};
use crate::error::CoreError;
use crate::models::{
    AuthCredentials, ClientConfig, EditPlaylistResult, MoveAction, MutationAction, Playlist,
    PlaylistPrivacy, PlaylistSummary, SessionStatus, Track,
};
use crate::parser::{
    check_innertube_error, normalize_playlist_id, parse_continuation_page,
    parse_edit_playlist_response, parse_library_playlists, parse_playlist_page,
    parse_search_tracks, parse_session_status, to_browse_playlist_id,
};
use crate::traits::{HttpTransport, InnerTubeClientTrait};

const SEARCH_SONGS_PARAMS: &str = "EgWKAQIIAWoMEA4QChADEAQQCRAF";

/// Resilient YouTube Music InnerTube client backed by any [`HttpTransport`].
pub struct InnerTubeClient<T: HttpTransport> {
    transport: T,
    credentials: AuthCredentials,
    config: ClientConfig,
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
        if let Some(page_id) = &self.credentials.x_goog_page_id {
            if !page_id.trim().is_empty() {
                user.insert("onBehalfOfUser".to_string(), Value::String(page_id.clone()));
            }
        }

        // `context.user` MUST sit as sibling of `context.client`, never nested inside `client`.
        json!({
            "client": {
                "clientName": "WEB_REMIX",
                "clientVersion": self.config.client_version,
                "hl": self.config.hl,
                "gl": self.config.gl,
            },
            "user": Value::Object(user),
        })
    }

    async fn post_authed(&self, endpoint: &str, mut payload: Value) -> Result<Value, CoreError> {
        let headers = build_auth_headers(&self.credentials, (self.now_fn)())?;
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("context".to_string(), self.build_context());
        }
        self.transport.post_json(endpoint, &payload, &headers).await
    }

    async fn send_edit_chunks(
        &self,
        playlist_id: &str,
        actions: &[MutationAction],
    ) -> Result<Vec<EditPlaylistResult>, CoreError> {
        let clean_id = normalize_playlist_id(playlist_id);
        if actions.is_empty() {
            return Ok(Vec::new());
        }
        let chunk_size = self.config.mutation_chunk_size.max(1);
        let chunks: Vec<&[MutationAction]> = actions.chunks(chunk_size).collect();
        let total = chunks.len();
        let mut results = Vec::with_capacity(total);

        for (idx, chunk) in chunks.into_iter().enumerate() {
            let action_values: Vec<Value> = chunk
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()
                .map_err(|e| CoreError::InvalidInput(e.to_string()))?;

            let resp = self
                .post_authed(
                    "browse/edit_playlist",
                    json!({
                        "playlistId": clean_id,
                        "actions": action_values,
                    }),
                )
                .await?;
            results.push(parse_edit_playlist_response(&resp)?);

            if idx + 1 < total && self.config.mutation_delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.config.mutation_delay_ms)).await;
            }
        }
        Ok(results)
    }
}

#[async_trait]
impl<T: HttpTransport> InnerTubeClientTrait for InnerTubeClient<T> {
    async fn verify_session(&self) -> Result<SessionStatus, CoreError> {
        let resp = self.post_authed("account/account_menu", json!({})).await?;
        let auth_user = self.credentials.x_goog_authuser.as_deref().unwrap_or("0");
        parse_session_status(&resp, auth_user, self.credentials.x_goog_page_id.as_deref())
    }

    async fn get_library_playlists(&self) -> Result<Vec<PlaylistSummary>, CoreError> {
        let resp = self
            .post_authed("browse", json!({ "browseId": "FEmusic_liked_playlists" }))
            .await?;
        check_innertube_error(&resp)?;
        Ok(parse_library_playlists(&resp))
    }

    async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist, CoreError> {
        let browse_id = to_browse_playlist_id(playlist_id);
        let first = self
            .post_authed("browse", json!({ "browseId": browse_id }))
            .await?;
        let (mut playlist, mut next_token) = parse_playlist_page(&first, playlist_id)?;

        let mut pages = 0usize;
        while let Some(token) = next_token {
            if pages >= self.config.max_pagination_pages {
                tracing::warn!(
                    playlist_id = %playlist.id,
                    max_pages = self.config.max_pagination_pages,
                    "pagination ceiling reached"
                );
                break;
            }
            let page = self
                .post_authed("browse", json!({ "continuation": token }))
                .await?;
            check_innertube_error(&page)?;
            let (more_tracks, next) = parse_continuation_page(&page, playlist.tracks.len());
            if more_tracks.is_empty() {
                break;
            }
            playlist.tracks.extend(more_tracks);
            next_token = next;
            pages += 1;
        }

        playlist.track_count = playlist.tracks.len();
        Ok(playlist)
    }

    async fn get_liked_songs(&self) -> Result<Playlist, CoreError> {
        self.get_playlist("LM").await
    }

    async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        video_ids: &[String],
    ) -> Result<String, CoreError> {
        if title.contains('<') || title.contains('>') {
            return Err(CoreError::InvalidInput(
                "playlist title cannot contain '<' or '>'".to_string(),
            ));
        }

        let mut body = json!({
            "title": title,
            "description": description,
            "privacyStatus": privacy,
        });
        if !video_ids.is_empty() {
            body.as_object_mut()
                .expect("object")
                .insert("videoIds".to_string(), json!(video_ids));
        }

        let resp = self.post_authed("playlist/create", body).await?;
        check_innertube_error(&resp)?;
        resp.get("playlistId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| CoreError::ParseError {
                context: "playlist/create".to_string(),
                detail: "missing playlistId in response".to_string(),
            })
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<(), CoreError> {
        let clean_id = normalize_playlist_id(playlist_id);
        let resp = self
            .post_authed("playlist/delete", json!({ "playlistId": clean_id }))
            .await?;
        let _ = parse_edit_playlist_response(&resp)?;
        Ok(())
    }

    async fn add_playlist_items(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<EditPlaylistResult, CoreError> {
        let actions: Vec<MutationAction> = video_ids
            .iter()
            .map(|vid| MutationAction::AddVideo {
                added_video_id: Some(vid.clone()),
                dedupe_option: allow_duplicates.then(|| "DEDUPE_OPTION_SKIP".to_string()),
            })
            .collect();

        let results = self.send_edit_chunks(playlist_id, &actions).await?;
        let mut combined = EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items: Vec::new(),
        };
        for res in results {
            combined.status = res.status;
            combined.added_items.extend(res.added_items);
        }
        Ok(combined)
    }

    async fn remove_playlist_items(
        &self,
        playlist_id: &str,
        items: &[(String, String)],
    ) -> Result<EditPlaylistResult, CoreError> {
        let mut actions = Vec::with_capacity(items.len());
        for (video_id, set_video_id) in items {
            if set_video_id.trim().is_empty() {
                return Err(CoreError::MissingSetVideoId {
                    video_id: video_id.clone(),
                });
            }
            actions.push(MutationAction::RemoveVideo {
                set_video_id: set_video_id.clone(),
                removed_video_id: video_id.clone(),
            });
        }

        let results = self.send_edit_chunks(playlist_id, &actions).await?;
        Ok(results.into_iter().last().unwrap_or(EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items: Vec::new(),
        }))
    }

    async fn move_playlist_items(
        &self,
        playlist_id: &str,
        moves: &[MoveAction],
    ) -> Result<Vec<EditPlaylistResult>, CoreError> {
        let actions: Vec<MutationAction> =
            moves.iter().cloned().map(MutationAction::from).collect();
        self.send_edit_chunks(playlist_id, &actions).await
    }

    async fn search_tracks(&self, query: &str, limit: usize) -> Result<Vec<Track>, CoreError> {
        let resp = self
            .post_authed(
                "search",
                json!({
                    "query": query,
                    "params": SEARCH_SONGS_PARAMS,
                }),
            )
            .await?;
        check_innertube_error(&resp)?;
        Ok(parse_search_tracks(&resp, limit))
    }
}
