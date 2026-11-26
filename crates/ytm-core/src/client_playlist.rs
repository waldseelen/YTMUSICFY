use std::collections::HashSet;

use chrono::Utc;
use serde_json::json;

use crate::client::InnerTubeClient;
use crate::error::CoreError;
use crate::fetch_models::{FetchManifest, PlaylistFetchResult};
use crate::http::RetryPolicy;
use crate::models::PlaylistSummary;
use crate::parser::{
    parse_continuation_page_detailed, parse_library_playlists, parse_playlist_page_detailed,
    to_browse_playlist_id,
};
use crate::traits::HttpTransport;

impl<T: HttpTransport> InnerTubeClient<T> {
    pub(crate) async fn fetch_library_playlists_paginated(
        &self,
    ) -> Result<Vec<PlaylistSummary>, CoreError> {
        let first = self
            .post_authed_with_policy(
                "browse",
                json!({ "browseId": "FEmusic_liked_playlists" }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        let (mut summaries, mut next_token) = parse_library_playlists(&first)?;

        let mut seen_tokens = HashSet::new();
        let mut pages_fetched = 1usize;
        let max_pages = self.config.max_pagination_pages.max(1);

        while let Some(token) = next_token {
            if pages_fetched >= max_pages {
                tracing::warn!(
                    pages_fetched,
                    max_pages,
                    "library pagination ceiling reached"
                );
                break;
            }
            if !seen_tokens.insert(token.clone()) {
                tracing::warn!(token = %token, "duplicate library continuation token detected");
                break;
            }
            let page = self
                .post_authed_with_policy(
                    "browse",
                    json!({ "continuation": token }),
                    RetryPolicy::IdempotentRead,
                )
                .await?;
            let (more, next) = parse_library_playlists(&page)?;
            summaries.extend(more);
            next_token = next;
            pages_fetched += 1;
        }

        Ok(summaries)
    }

    pub(crate) async fn fetch_playlist_with_manifest_impl(
        &self,
        playlist_id: &str,
    ) -> Result<PlaylistFetchResult, CoreError> {
        let browse_id = to_browse_playlist_id(playlist_id);
        let first = self
            .post_authed_with_policy(
                "browse",
                json!({ "browseId": browse_id }),
                RetryPolicy::IdempotentRead,
            )
            .await?;
        let parsed = parse_playlist_page_detailed(&first, playlist_id)?;
        let mut playlist = parsed.playlist;
        let declared_track_count = parsed.declared_track_count;
        let mut dropped_renderers = parsed.dropped_renderers;
        let mut next_token = parsed.next_token;

        let mut seen_tokens = HashSet::new();
        let mut pages_fetched = 1usize;
        let max_pages = self.config.max_pagination_pages.max(1);
        let mut truncated = false;

        while let Some(token) = next_token {
            if pages_fetched >= max_pages || !seen_tokens.insert(token.clone()) {
                tracing::warn!(
                    playlist_id = %playlist.id,
                    pages_fetched,
                    max_pages,
                    "playlist pagination stopped before completion"
                );
                truncated = true;
                break;
            }
            let page = self
                .post_authed_with_policy(
                    "browse",
                    json!({ "continuation": token }),
                    RetryPolicy::IdempotentRead,
                )
                .await?;
            let cont = parse_continuation_page_detailed(&page, playlist.tracks.len())?;
            playlist.tracks.extend(cont.tracks);
            dropped_renderers.extend(cont.dropped_renderers);
            next_token = cont.next_token;
            pages_fetched += 1;
        }

        playlist.track_count = playlist.tracks.len();
        let manifest = FetchManifest::build(
            &playlist,
            declared_track_count,
            pages_fetched,
            truncated,
            self.config.max_pagination_pages,
            dropped_renderers,
            Utc::now(),
        );

        Ok(PlaylistFetchResult { playlist, manifest })
    }
}
