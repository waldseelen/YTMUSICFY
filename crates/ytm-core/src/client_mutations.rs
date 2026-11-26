use std::time::Duration;

use serde_json::{Value, json};

use crate::client::InnerTubeClient;
use crate::error::CoreError;
use crate::fetch_models::{ChunkOutcome, MutationBatchReport};
use crate::http::{RetryPolicy, jittered_delay_ms};
use crate::models::{MutationAction, PlaylistPrivacy};
use crate::parser::{normalize_playlist_id, parse_edit_playlist_response};
use crate::traits::HttpTransport;

fn is_transient_post_create_error(err: &CoreError) -> bool {
    match err {
        CoreError::HttpError(msg) => msg.contains("HTTP 409") || msg.contains("HTTP 400"),
        _ => false,
    }
}

impl<T: HttpTransport> InnerTubeClient<T> {
    pub(crate) async fn create_playlist_chunked(
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

        let chunk_size = self.config.mutation_chunk_size.max(1);
        let (first_chunk, remaining) = if video_ids.len() > chunk_size {
            (&video_ids[..chunk_size], &video_ids[chunk_size..])
        } else {
            (video_ids, &[][..])
        };

        let mut body = json!({
            "title": title,
            "description": description,
            "privacyStatus": privacy,
        });
        if !first_chunk.is_empty() {
            body.as_object_mut()
                .expect("object")
                .insert("videoIds".to_string(), json!(first_chunk));
        }

        let resp = self
            .post_authed_with_policy("playlist/create", body, RetryPolicy::MutationSafe)
            .await?;
        let playlist_id = resp
            .get("playlistId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| CoreError::ParseError {
                context: "playlist/create".to_string(),
                detail: "missing playlistId in response".to_string(),
            })?;

        if !remaining.is_empty() {
            self.append_post_create_chunks(&playlist_id, remaining, chunk_size)
                .await?;
        }
        Ok(playlist_id)
    }

    async fn append_post_create_chunks(
        &self,
        playlist_id: &str,
        remaining: &[String],
        chunk_size: usize,
    ) -> Result<(), CoreError> {
        let clean_id = normalize_playlist_id(playlist_id);
        let chunks: Vec<&[String]> = remaining.chunks(chunk_size).collect();
        let total = chunks.len();

        for (idx, chunk) in chunks.into_iter().enumerate() {
            let actions: Vec<MutationAction> = chunk
                .iter()
                .map(|vid| MutationAction::AddVideo {
                    added_video_id: Some(vid.clone()),
                    dedupe_option: Some("DEDUPE_OPTION_SKIP".to_string()),
                })
                .collect();
            let action_values: Vec<Value> = actions
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()
                .map_err(|e| CoreError::InvalidInput(e.to_string()))?;

            let mut attempt = 0usize;
            loop {
                let res = self
                    .post_authed_with_policy(
                        "browse/edit_playlist",
                        json!({
                            "playlistId": clean_id,
                            "actions": action_values,
                        }),
                        RetryPolicy::MutationSafe,
                    )
                    .await;
                match res.and_then(|resp| parse_edit_playlist_response(&resp)) {
                    Ok(_) => break,
                    Err(err)
                        if is_transient_post_create_error(&err)
                            && attempt < self.config.max_retries =>
                    {
                        if self.config.mutation_delay_ms > 0 {
                            let exp = 2u64.saturating_pow(u32::try_from(attempt).unwrap_or(4));
                            let base = self.config.mutation_delay_ms.saturating_mul(exp).min(8_000);
                            let delay = jittered_delay_ms(base, attempt);
                            tokio::time::sleep(Duration::from_millis(delay)).await;
                        }
                        attempt += 1;
                    }
                    Err(err) => return Err(err),
                }
            }

            if idx + 1 < total && self.config.mutation_delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.config.mutation_delay_ms)).await;
            }
        }
        Ok(())
    }

    pub(crate) async fn send_edit_chunks_with_report(
        &self,
        playlist_id: &str,
        actions: &[MutationAction],
    ) -> (MutationBatchReport, Option<CoreError>) {
        let clean_id = normalize_playlist_id(playlist_id);
        if actions.is_empty() {
            return (
                MutationBatchReport {
                    total_chunks: 0,
                    outcomes: Vec::new(),
                },
                None,
            );
        }

        let chunk_size = self.config.mutation_chunk_size.max(1);
        let chunks: Vec<&[MutationAction]> = actions.chunks(chunk_size).collect();
        let total_chunks = chunks.len();
        let mut outcomes = Vec::with_capacity(total_chunks);

        for (chunk_index, chunk) in chunks.into_iter().enumerate() {
            let action_count = chunk.len();
            let action_values: Vec<Value> = match chunk
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()
            {
                Ok(vals) => vals,
                Err(e) => {
                    let err = CoreError::InvalidInput(e.to_string());
                    outcomes.push(ChunkOutcome::Failed {
                        chunk_index,
                        action_count,
                        error: err.to_string(),
                    });
                    return (
                        MutationBatchReport {
                            total_chunks,
                            outcomes,
                        },
                        Some(err),
                    );
                }
            };

            let resp = self
                .post_authed_with_policy(
                    "browse/edit_playlist",
                    json!({
                        "playlistId": clean_id,
                        "actions": action_values,
                    }),
                    RetryPolicy::MutationSafe,
                )
                .await;

            match resp.and_then(|val| parse_edit_playlist_response(&val)) {
                Ok(result) => outcomes.push(ChunkOutcome::Succeeded {
                    chunk_index,
                    action_count,
                    result,
                }),
                Err(err) => {
                    outcomes.push(ChunkOutcome::Failed {
                        chunk_index,
                        action_count,
                        error: err.to_string(),
                    });
                    return (
                        MutationBatchReport {
                            total_chunks,
                            outcomes,
                        },
                        Some(err),
                    );
                }
            }

            if chunk_index + 1 < total_chunks && self.config.mutation_delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.config.mutation_delay_ms)).await;
            }
        }

        (
            MutationBatchReport {
                total_chunks,
                outcomes,
            },
            None,
        )
    }

    pub(crate) fn build_add_actions(
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Vec<MutationAction> {
        video_ids
            .iter()
            .map(|vid| MutationAction::AddVideo {
                added_video_id: Some(vid.clone()),
                dedupe_option: allow_duplicates.then(|| "DEDUPE_OPTION_SKIP".to_string()),
            })
            .collect()
    }

    pub(crate) fn build_remove_actions(
        items: &[(String, String)],
    ) -> Result<Vec<MutationAction>, CoreError> {
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
        Ok(actions)
    }

    pub(crate) async fn update_playlist_metadata_impl(
        &self,
        playlist_id: &str,
        title: Option<&str>,
        description: Option<&str>,
        privacy: Option<PlaylistPrivacy>,
    ) -> Result<(), CoreError> {
        let mut actions = Vec::new();
        if let Some(name) = title {
            if name.contains('<') || name.contains('>') {
                return Err(CoreError::InvalidInput(
                    "playlist title cannot contain '<' or '>'".to_string(),
                ));
            }
            actions.push(MutationAction::SetPlaylistName {
                playlist_name: name.to_string(),
            });
        }
        if let Some(desc) = description {
            actions.push(MutationAction::SetPlaylistDescription {
                playlist_description: desc.to_string(),
            });
        }
        if let Some(priv_status) = privacy {
            actions.push(MutationAction::SetPlaylistPrivacy {
                playlist_privacy: priv_status,
            });
        }
        let (_, err) = self
            .send_edit_chunks_with_report(playlist_id, &actions)
            .await;
        if let Some(first_err) = err {
            return Err(first_err);
        }
        Ok(())
    }
}
