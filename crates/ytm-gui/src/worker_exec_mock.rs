use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

use chrono::Utc;
use ytm_core::{
    AddedPlaylistItemResult, BrowsePage, ChunkOutcome, CoreError, EditPlaylistResult,
    FetchCompleteness, FetchManifest, InnerTubeClientTrait, MoveAction, MusicVideoType,
    MutationBatchReport, Playlist, PlaylistFetchResult, PlaylistPrivacy, PlaylistSummary,
    SessionStatus, Track, TrackAvailability, WatchNextResult,
};

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub(super) fn mk_track(vid: &str, sid: Option<&str>, pos: usize) -> Track {
    Track {
        video_id: vid.to_string(),
        set_video_id: sid.map(str::to_string),
        position: pos,
        title: format!("Title {vid}"),
        artists: Vec::new(),
        album: None,
        duration_sec: Some(180),
        release_year: None,
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url: None,
    }
}

pub(super) fn mk_playlist(id: &str, tracks: Vec<Track>) -> Playlist {
    Playlist {
        id: id.to_string(),
        title: format!("Playlist {id}"),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: tracks.len(),
        tracks,
    }
}

#[derive(Default)]
pub(super) struct MockClient {
    pub(super) playlists: Mutex<HashMap<String, PlaylistFetchResult>>,
    pub(super) fail_add_on: Mutex<Option<String>>,
    pub(super) empty_add_items: Mutex<bool>,
    pub(super) calls: Mutex<Vec<String>>,
    pub(super) log_fetches: Mutex<bool>,
}

impl MockClient {
    pub(super) fn insert(&self, pl: Playlist, completeness: FetchCompleteness) {
        let mut manifest = FetchManifest::from_playlist_complete(&pl);
        manifest.completeness = completeness;
        manifest.fetched_at = Utc::now();
        self.playlists.lock().unwrap().insert(
            pl.id.clone(),
            PlaylistFetchResult {
                playlist: pl,
                manifest,
            },
        );
    }
}

impl InnerTubeClientTrait for MockClient {
    fn verify_session<'life0, 'async_trait>(
        &'life0 self,
    ) -> BoxFut<'async_trait, Result<SessionStatus, CoreError>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        unreachable!("not used in worker_exec tests")
    }

    fn get_library_playlists<'life0, 'async_trait>(
        &'life0 self,
    ) -> BoxFut<'async_trait, Result<Vec<PlaylistSummary>, CoreError>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn get_playlist<'life0, 'life1, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
    ) -> BoxFut<'async_trait, Result<Playlist, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.playlists
                .lock()
                .unwrap()
                .get(id)
                .map(|r| r.playlist.clone())
                .ok_or_else(|| CoreError::ParseError {
                    context: "mock".to_string(),
                    detail: format!("missing {id}"),
                })
        })
    }

    fn get_liked_songs<'life0, 'async_trait>(
        &'life0 self,
    ) -> BoxFut<'async_trait, Result<Playlist, CoreError>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        self.get_playlist("LM")
    }

    fn get_playlist_with_manifest<'life0, 'life1, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
    ) -> BoxFut<'async_trait, Result<PlaylistFetchResult, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            if *self.log_fetches.lock().unwrap() {
                self.calls.lock().unwrap().push(format!("fetch:{id}"));
            }
            self.playlists
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .ok_or_else(|| CoreError::ParseError {
                    context: "mock".to_string(),
                    detail: format!("missing {id}"),
                })
        })
    }

    fn create_playlist<'life0, 'life1, 'life2, 'life3, 'async_trait>(
        &'life0 self,
        title: &'life1 str,
        _: &'life2 str,
        _: PlaylistPrivacy,
        _: &'life3 [String],
    ) -> BoxFut<'async_trait, Result<String, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.calls.lock().unwrap().push(format!("create:{title}"));
            if self.fail_add_on.lock().unwrap().as_deref() == Some(title) {
                return Err(CoreError::ApiError {
                    code: "MOCK".to_string(),
                    message: "create failed".to_string(),
                });
            }
            Ok(format!("NEW_{title}"))
        })
    }

    fn delete_playlist<'life0, 'life1, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
    ) -> BoxFut<'async_trait, Result<(), CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.calls.lock().unwrap().push(format!("delete:{id}"));
            Ok(())
        })
    }

    fn add_playlist_items<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
        vids: &'life2 [String],
        _: bool,
    ) -> BoxFut<'async_trait, Result<EditPlaylistResult, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("add:{id}:{}", vids.len()));
            if self.fail_add_on.lock().unwrap().as_deref() == Some(id) {
                return Err(CoreError::ApiError {
                    code: "MOCK".to_string(),
                    message: "add failed".to_string(),
                });
            }
            let added_items = if *self.empty_add_items.lock().unwrap() {
                Vec::new()
            } else {
                vids.iter()
                    .enumerate()
                    .map(|(i, v)| AddedPlaylistItemResult {
                        video_id: v.clone(),
                        set_video_id: format!("NEW_SID_{i}"),
                    })
                    .collect()
            };
            Ok(EditPlaylistResult {
                status: "STATUS_SUCCEEDED".to_string(),
                added_items,
            })
        })
    }

    fn remove_playlist_items<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
        items: &'life2 [(String, String)],
    ) -> BoxFut<'async_trait, Result<EditPlaylistResult, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("remove:{id}:{}", items.len()));
            Ok(EditPlaylistResult {
                status: "STATUS_SUCCEEDED".to_string(),
                added_items: Vec::new(),
            })
        })
    }

    fn move_playlist_items<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
        moves: &'life2 [MoveAction],
    ) -> BoxFut<'async_trait, Result<Vec<EditPlaylistResult>, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("move:{id}:{}", moves.len()));
            Ok(vec![EditPlaylistResult {
                status: "STATUS_SUCCEEDED".to_string(),
                added_items: Vec::new(),
            }])
        })
    }

    fn move_playlist_items_report<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        id: &'life1 str,
        moves: &'life2 [MoveAction],
    ) -> BoxFut<'async_trait, Result<MutationBatchReport, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            let res = self.move_playlist_items(id, moves).await?;
            Ok(MutationBatchReport {
                total_chunks: 1,
                outcomes: vec![ChunkOutcome::Succeeded {
                    chunk_index: 0,
                    action_count: moves.len(),
                    result: res.into_iter().next().unwrap(),
                }],
            })
        })
    }

    fn search_tracks<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _: &'life1 str,
        _: usize,
    ) -> BoxFut<'async_trait, Result<Vec<Track>, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn update_playlist_metadata<'life0, 'life1, 'life2, 'life3, 'async_trait>(
        &'life0 self,
        _: &'life1 str,
        _: Option<&'life2 str>,
        _: Option<&'life3 str>,
        _: Option<PlaylistPrivacy>,
    ) -> BoxFut<'async_trait, Result<(), CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        'life3: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }

    fn set_like_status<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _: &'life1 str,
        _: bool,
    ) -> BoxFut<'async_trait, Result<(), CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(()) })
    }

    fn get_browse_page<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _: &'life1 str,
    ) -> BoxFut<'async_trait, Result<BrowsePage, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        unreachable!()
    }

    fn get_watch_next<'life0, 'life1, 'life2, 'async_trait>(
        &'life0 self,
        _: &'life1 str,
        _: Option<&'life2 str>,
    ) -> BoxFut<'async_trait, Result<WatchNextResult, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        'life2: 'async_trait,
        Self: 'async_trait,
    {
        unreachable!()
    }

    fn get_lyrics<'life0, 'life1, 'async_trait>(
        &'life0 self,
        _: &'life1 str,
    ) -> BoxFut<'async_trait, Result<Option<String>, CoreError>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async { Ok(None) })
    }
}
