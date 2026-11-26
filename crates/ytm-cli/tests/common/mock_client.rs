use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use ytm_cli::SessionVerifier;
use ytm_core::{
    AddedPlaylistItemResult, ArtistRef, AuthCredentials, BrowsePage, BrowsePageKind, ChunkOutcome,
    ClientConfig, CoreError, EditPlaylistResult, FetchManifest, InnerTubeClientTrait, MoveAction,
    MusicVideoType, MutationBatchReport, Playlist, PlaylistFetchResult, PlaylistPrivacy,
    PlaylistSummary, SessionStatus, Track, TrackAvailability, WatchNextResult,
};

pub struct StatefulMockClient {
    pub playlists: Mutex<HashMap<String, Playlist>>,
    pub manifests: Mutex<HashMap<String, FetchManifest>>,
    pub skip_add_confirm: Mutex<HashSet<String>>,
    pub search_catalog: HashMap<String, Vec<Track>>,
    pub mutation_count: AtomicUsize,
    next_id: AtomicUsize,
}

impl StatefulMockClient {
    pub fn new(playlists: Vec<Playlist>, search_catalog: HashMap<String, Vec<Track>>) -> Self {
        let map = playlists.into_iter().map(|p| (p.id.clone(), p)).collect();
        Self {
            playlists: Mutex::new(map),
            manifests: Mutex::new(HashMap::new()),
            skip_add_confirm: Mutex::new(HashSet::new()),
            search_catalog,
            mutation_count: AtomicUsize::new(0),
            next_id: AtomicUsize::new(100),
        }
    }

    pub fn mutations(&self) -> usize {
        self.mutation_count.load(Ordering::SeqCst)
    }

    fn alloc_id(&self, prefix: &str) -> String {
        let n = self.next_id.fetch_add(1, Ordering::SeqCst);
        format!("{prefix}_{n}")
    }
}

#[async_trait]
impl InnerTubeClientTrait for StatefulMockClient {
    async fn verify_session(&self) -> Result<SessionStatus, CoreError> {
        Ok(SessionStatus {
            authenticated: true,
            account_name: Some("Test Architect".to_string()),
            channel_handle: Some("@architect".to_string()),
            auth_user: "0".to_string(),
            brand_page_id: None,
        })
    }

    async fn get_library_playlists(&self) -> Result<Vec<PlaylistSummary>, CoreError> {
        let guard = self.playlists.lock().expect("lock playlists");
        let mut out: Vec<PlaylistSummary> = guard
            .values()
            .map(|pl| PlaylistSummary {
                id: pl.id.clone(),
                title: pl.title.clone(),
                track_count: Some(pl.tracks.len()),
                owned: pl.owned,
                is_liked_music: pl.id == "LM",
            })
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    async fn get_playlist(&self, playlist_id: &str) -> Result<Playlist, CoreError> {
        let guard = self.playlists.lock().expect("lock playlists");
        guard
            .get(playlist_id)
            .cloned()
            .ok_or_else(|| CoreError::InvalidInput(format!("unknown playlist {playlist_id}")))
    }

    async fn get_liked_songs(&self) -> Result<Playlist, CoreError> {
        self.get_playlist("LM").await
    }

    async fn get_playlist_with_manifest(
        &self,
        playlist_id: &str,
    ) -> Result<PlaylistFetchResult, CoreError> {
        let playlist = self.get_playlist(playlist_id).await?;
        let manifest = self
            .manifests
            .lock()
            .expect("lock manifests")
            .get(playlist_id)
            .cloned()
            .unwrap_or_else(|| FetchManifest::from_playlist_complete(&playlist));
        Ok(PlaylistFetchResult { playlist, manifest })
    }

    async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        video_ids: &[String],
    ) -> Result<String, CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        let id = self.alloc_id("PL_NEW");
        let tracks: Vec<Track> = video_ids
            .iter()
            .enumerate()
            .map(|(i, vid)| Track {
                video_id: vid.clone(),
                set_video_id: Some(self.alloc_id("SET")),
                position: i,
                title: format!("Track {vid}"),
                artists: vec![ArtistRef {
                    name: "Artist".to_string(),
                    browse_id: None,
                }],
                album: None,
                duration_sec: Some(200),
                release_year: Some(2020),
                music_video_type: MusicVideoType::AudioTrack,
                availability: TrackAvailability::Available,
                thumbnail_url: None,
            })
            .collect();
        let pl = Playlist {
            id: id.clone(),
            title: title.to_string(),
            description: Some(description.to_string()),
            privacy,
            owned: true,
            track_count: tracks.len(),
            tracks,
        };
        self.playlists
            .lock()
            .expect("lock playlists")
            .insert(id.clone(), pl);
        Ok(id)
    }

    async fn delete_playlist(&self, playlist_id: &str) -> Result<(), CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        self.playlists
            .lock()
            .expect("lock playlists")
            .remove(playlist_id);
        Ok(())
    }

    async fn add_playlist_items(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<EditPlaylistResult, CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        let skip = self
            .skip_add_confirm
            .lock()
            .expect("lock skip_add_confirm")
            .clone();
        let mut guard = self.playlists.lock().expect("lock playlists");
        let pl = guard
            .get_mut(playlist_id)
            .ok_or_else(|| CoreError::InvalidInput("missing playlist".to_string()))?;

        let mut added_items = Vec::new();
        for vid in video_ids {
            if skip.contains(vid)
                || (!allow_duplicates && pl.tracks.iter().any(|t| &t.video_id == vid))
            {
                continue;
            }
            let sid = self.alloc_id("SET");
            let pos = pl.tracks.len();
            pl.tracks.push(Track {
                video_id: vid.clone(),
                set_video_id: Some(sid.clone()),
                position: pos,
                title: format!("Track {vid}"),
                artists: vec![ArtistRef {
                    name: "Added Artist".to_string(),
                    browse_id: None,
                }],
                album: None,
                duration_sec: Some(210),
                release_year: Some(2020),
                music_video_type: MusicVideoType::AudioTrack,
                availability: TrackAvailability::Available,
                thumbnail_url: None,
            });
            added_items.push(AddedPlaylistItemResult {
                video_id: vid.clone(),
                set_video_id: sid,
            });
        }
        pl.track_count = pl.tracks.len();
        Ok(EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items,
        })
    }

    async fn add_playlist_items_report(
        &self,
        playlist_id: &str,
        video_ids: &[String],
        allow_duplicates: bool,
    ) -> Result<MutationBatchReport, CoreError> {
        let res = self
            .add_playlist_items(playlist_id, video_ids, allow_duplicates)
            .await?;
        Ok(MutationBatchReport {
            total_chunks: 1,
            outcomes: vec![ChunkOutcome::Succeeded {
                chunk_index: 0,
                action_count: video_ids.len(),
                result: res,
            }],
        })
    }

    async fn remove_playlist_items(
        &self,
        playlist_id: &str,
        items: &[(String, String)],
    ) -> Result<EditPlaylistResult, CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        let mut guard = self.playlists.lock().expect("lock playlists");
        let pl = guard
            .get_mut(playlist_id)
            .ok_or_else(|| CoreError::InvalidInput("missing playlist".to_string()))?;
        pl.tracks.retain(|t| {
            let sid = t.set_video_id.as_deref().unwrap_or("");
            !items.iter().any(|(vid, s)| vid == &t.video_id && s == sid)
        });
        for (i, t) in pl.tracks.iter_mut().enumerate() {
            t.position = i;
        }
        pl.track_count = pl.tracks.len();
        Ok(EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items: Vec::new(),
        })
    }

    async fn move_playlist_items(
        &self,
        playlist_id: &str,
        moves: &[MoveAction],
    ) -> Result<Vec<EditPlaylistResult>, CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        let mut guard = self.playlists.lock().expect("lock playlists");
        let pl = guard
            .get_mut(playlist_id)
            .ok_or_else(|| CoreError::InvalidInput("missing playlist".to_string()))?;

        for mv in moves {
            if let Some(from) = pl
                .tracks
                .iter()
                .position(|t| t.set_video_id.as_deref() == Some(mv.set_video_id.as_str()))
            {
                let item = pl.tracks.remove(from);
                match &mv.moved_set_video_successor {
                    Some(succ) => {
                        if let Some(to) = pl
                            .tracks
                            .iter()
                            .position(|t| t.set_video_id.as_deref() == Some(succ.as_str()))
                        {
                            pl.tracks.insert(to, item);
                        } else {
                            pl.tracks.push(item);
                        }
                    }
                    None => pl.tracks.push(item),
                }
            }
        }
        for (i, t) in pl.tracks.iter_mut().enumerate() {
            t.position = i;
        }
        Ok(vec![EditPlaylistResult {
            status: "STATUS_SUCCEEDED".to_string(),
            added_items: Vec::new(),
        }])
    }

    async fn search_tracks(&self, query: &str, _limit: usize) -> Result<Vec<Track>, CoreError> {
        let lower = query.to_ascii_lowercase();
        for (needle, res) in &self.search_catalog {
            if lower.contains(needle) {
                return Ok(res.clone());
            }
        }
        Ok(Vec::new())
    }

    async fn update_playlist_metadata(
        &self,
        playlist_id: &str,
        title: Option<&str>,
        description: Option<&str>,
        privacy: Option<PlaylistPrivacy>,
    ) -> Result<(), CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        let mut guard = self.playlists.lock().expect("lock playlists");
        let pl = guard
            .get_mut(playlist_id)
            .ok_or_else(|| CoreError::InvalidInput("missing playlist".to_string()))?;
        if let Some(t) = title {
            pl.title = t.to_string();
        }
        if let Some(d) = description {
            pl.description = Some(d.to_string());
        }
        if let Some(p) = privacy {
            pl.privacy = p;
        }
        Ok(())
    }

    async fn set_like_status(&self, _video_id: &str, _liked: bool) -> Result<(), CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn get_browse_page(&self, browse_id: &str) -> Result<BrowsePage, CoreError> {
        Ok(BrowsePage {
            browse_id: browse_id.to_string(),
            kind: BrowsePageKind::Home,
            title: "Mock Browse".to_string(),
            subtitle: None,
            thumbnail_url: None,
            sections: Vec::new(),
            tracks: Vec::new(),
            continuation: None,
        })
    }

    async fn get_watch_next(
        &self,
        _video_id: &str,
        _playlist_id: Option<&str>,
    ) -> Result<WatchNextResult, CoreError> {
        Ok(WatchNextResult {
            tracks: Vec::new(),
            lyrics_browse_id: None,
        })
    }

    async fn get_lyrics(&self, _lyrics_browse_id: &str) -> Result<Option<String>, CoreError> {
        Ok(None)
    }
}

pub struct MockVerifier;

#[async_trait]
impl SessionVerifier for MockVerifier {
    async fn verify_credentials(
        &self,
        _credentials: &AuthCredentials,
        _config: &ClientConfig,
    ) -> Result<SessionStatus> {
        Ok(SessionStatus {
            authenticated: true,
            account_name: Some("Mock User".to_string()),
            channel_handle: Some("@mock".to_string()),
            auth_user: "0".to_string(),
            brand_page_id: None,
        })
    }
}
