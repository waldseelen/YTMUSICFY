use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;
use tempfile::TempDir;
use ytm_cli::{
    ActiveTab, AppConfig, AuthCommands, CliExportFormat, CliKeepStrategy, CliSetOp, Commands,
    DedupeArgs, HealthArgs, RuntimeContext, SessionVerifier, SetArgs, SnapshotCommands, SortArgs,
    SplitGenreArgs, TuiAction, TuiApp, execute_command,
};
use ytm_core::{
    AddedPlaylistItemResult, AlbumRef, ArtistRef, AuthCredentials, ClientConfig, CoreError,
    EditPlaylistResult, InnerTubeClientTrait, MoveAction, MusicVideoType, Playlist,
    PlaylistPrivacy, PlaylistSummary, SessionStatus, Track, TrackAvailability,
};
use ytm_enrich::{EnrichmentConfig, EnrichmentService, GenreMatcher, MockEnrichmentHttp};
use ytm_store::{AppPaths, SnapshotStore, SqliteStore};

struct StatefulMockClient {
    playlists: Mutex<HashMap<String, Playlist>>,
    search_catalog: HashMap<String, Vec<Track>>,
    mutation_count: AtomicUsize,
    next_id: AtomicUsize,
}

impl StatefulMockClient {
    fn new(playlists: Vec<Playlist>, search_catalog: HashMap<String, Vec<Track>>) -> Self {
        let map = playlists.into_iter().map(|p| (p.id.clone(), p)).collect();
        Self {
            playlists: Mutex::new(map),
            search_catalog,
            mutation_count: AtomicUsize::new(0),
            next_id: AtomicUsize::new(100),
        }
    }

    fn mutations(&self) -> usize {
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

    async fn create_playlist(
        &self,
        title: &str,
        description: &str,
        privacy: PlaylistPrivacy,
        video_ids: &[String],
    ) -> Result<String, CoreError> {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
        let id = self.alloc_id("PL_NEW");
        let tracks = video_ids
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
            })
            .collect::<Vec<_>>();
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
        let mut guard = self.playlists.lock().expect("lock playlists");
        let pl = guard
            .get_mut(playlist_id)
            .ok_or_else(|| CoreError::InvalidInput("missing playlist".to_string()))?;

        let mut added_items = Vec::new();
        for vid in video_ids {
            if !allow_duplicates && pl.tracks.iter().any(|t| &t.video_id == vid) {
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
}

struct MockVerifier;

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

fn track(
    vid: &str,
    sid: &str,
    pos: usize,
    artist: &str,
    title: &str,
    dur: u32,
    mv: MusicVideoType,
) -> Track {
    Track {
        video_id: vid.to_string(),
        set_video_id: Some(sid.to_string()),
        position: pos,
        title: title.to_string(),
        artists: if artist.is_empty() {
            Vec::new()
        } else {
            vec![ArtistRef {
                name: artist.to_string(),
                browse_id: None,
            }]
        },
        album: Some(AlbumRef {
            name: "Album".to_string(),
            browse_id: None,
        }),
        duration_sec: Some(dur),
        release_year: Some(2015),
        music_video_type: mv,
        availability: TrackAvailability::Available,
    }
}

#[tokio::test]
async fn e2e_cli_dry_run_mutate_snapshot_restore_and_idempotent_genre_split() {
    let dir = TempDir::new().expect("temp dir");
    let paths = AppPaths::from_root(dir.path());
    paths.ensure_dirs().expect("ensure dirs");
    let config = AppConfig::default();

    let pl_main = Playlist {
        id: "PL_MAIN".to_string(),
        title: "Architect Mix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 4,
        tracks: vec![
            track(
                "V_ZEB",
                "S_1",
                0,
                "Zebrahead",
                "Anthem",
                210,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_UGC",
                "S_2",
                1,
                "FanChan",
                "Daft Punk - Get Lucky (Lyrics)",
                248,
                MusicVideoType::UserGenerated,
            ),
            track(
                "V_ATV",
                "S_3",
                2,
                "Daft Punk",
                "Get Lucky",
                249,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_ABBA",
                "S_4",
                3,
                "ABBA",
                "Waterloo",
                168,
                MusicVideoType::AudioTrack,
            ),
        ],
    };
    let pl_other = Playlist {
        id: "PL_OTHER".to_string(),
        title: "Other Mix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 1,
        tracks: vec![track(
            "V_UNIQUE",
            "SO_1",
            0,
            "Muse",
            "Starlight",
            240,
            MusicVideoType::AudioTrack,
        )],
    };

    let client = StatefulMockClient::new(vec![pl_main, pl_other], HashMap::new());
    let store = Arc::new(SqliteStore::open_in_memory(Some(paths.snapshots_dir())).expect("store"));

    let mb_rock = json!({ "recordings": [{ "first-release-date": "2010", "tags": [{ "name": "rock", "count": 5 }] }] });
    let mb_elec = json!({ "recordings": [{ "first-release-date": "2013", "tags": [{ "name": "house", "count": 5 }] }] });
    let http = MockEnrichmentHttp::with_responses(vec![
        Ok((200, mb_rock.clone())),
        Ok((200, mb_elec)),
        Ok((200, mb_rock)),
    ]);
    let enrich_cfg = EnrichmentConfig {
        musicbrainz_interval_ms: 0,
        enable_deezer_fallback: false,
        ..Default::default()
    };
    let enricher = EnrichmentService::new(
        Arc::clone(&store),
        http,
        enrich_cfg,
        GenreMatcher::built_in_default(),
    );
    let verifier = MockVerifier;

    // 1. auth import writes auth.json securely
    let mut out = Vec::new();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: Some(
                json!({
                    "cookie": "SAPISID=abc; __Secure-3PAPISID=xyz",
                    "user-agent": "TestUA"
                })
                .to_string(),
            ),
            out: &mut out,
        };
        execute_command(
            &Commands::Auth {
                action: AuthCommands::Import { file: None },
            },
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("auth import");
    }
    assert!(paths.auth_file().exists());
    assert_eq!(client.mutations(), 0);

    // 2. dedupe dry-run emits ZERO mutations and ZERO snapshots
    out.clear();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Dedupe(DedupeArgs {
                playlist: "PL_MAIN".to_string(),
                threshold: 0.90,
                duration_tol: 15,
                ignore_duration: false,
                exact_only: false,
                keep: CliKeepStrategy::Quality,
                execute: false,
            }),
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("dedupe dry-run");
    }
    assert_eq!(client.mutations(), 0);
    assert!(store.list_snapshots(None).await.expect("snaps").is_empty());
    assert!(String::from_utf8_lossy(&out).contains("[DRY-RUN]"));

    // 3. dedupe --execute creates pre-dedupe snapshot & keeps ATV
    out.clear();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Dedupe(DedupeArgs {
                playlist: "PL_MAIN".to_string(),
                threshold: 0.90,
                duration_tol: 15,
                ignore_duration: false,
                exact_only: false,
                keep: CliKeepStrategy::Quality,
                execute: true,
            }),
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("dedupe execute");
    }
    let snaps_after_dedupe = store.list_snapshots(Some("PL_MAIN")).await.expect("snaps");
    assert_eq!(snaps_after_dedupe.len(), 1);
    assert_eq!(snaps_after_dedupe[0].reason, "pre-dedupe");
    let pre_dedupe_id = snaps_after_dedupe[0].id.clone();

    let after_dedupe = client.get_playlist("PL_MAIN").await.expect("pl");
    assert_eq!(after_dedupe.tracks.len(), 3);
    assert!(after_dedupe.tracks.iter().all(|t| t.video_id != "V_UGC"));

    // 4. sort --by artist:asc --execute sorts ABBA -> Daft Punk -> Zebrahead via LIS moves
    out.clear();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Sort(SortArgs {
                playlist: "PL_MAIN".to_string(),
                by: "artist:asc".to_string(),
                seed: None,
                execute: true,
            }),
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("sort execute");
    }
    let after_sort = client.get_playlist("PL_MAIN").await.expect("sorted pl");
    let ids_sorted: Vec<&str> = after_sort
        .tracks
        .iter()
        .map(|t| t.video_id.as_str())
        .collect();
    assert_eq!(ids_sorted, vec!["V_ABBA", "V_ATV", "V_ZEB"]);

    // 5. snapshot restore --execute restores exact original 4 tracks & original order
    out.clear();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Snapshots {
                action: SnapshotCommands::Restore {
                    snapshot_id: pre_dedupe_id,
                    execute: true,
                },
            },
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("restore execute");
    }
    let restored = client.get_playlist("PL_MAIN").await.expect("restored pl");
    let ids_restored: Vec<&str> = restored
        .tracks
        .iter()
        .map(|t| t.video_id.as_str())
        .collect();
    assert_eq!(ids_restored, vec!["V_ZEB", "V_UGC", "V_ATV", "V_ABBA"]);

    // 6. set diff --into PL_MAIN --execute inserts only V_UNIQUE
    out.clear();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Set(SetArgs {
                op: CliSetOp::Diff,
                playlist_a: "PL_OTHER".to_string(),
                playlist_b: "PL_MAIN".to_string(),
                fuzzy: true,
                into: Some("PL_MAIN".to_string()),
                execute: true,
            }),
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("set diff execute");
    }
    let after_set = client.get_playlist("PL_MAIN").await.expect("pl after set");
    assert_eq!(after_set.tracks.len(), 5);

    // 7. backup --format csv --out creates exported CSV file
    let export_dir = dir.path().join("exports");
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Backup {
                target: "PL_MAIN".to_string(),
                format: CliExportFormat::Csv,
                out: Some(export_dir.clone()),
            },
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("backup csv");
    }
    assert!(export_dir.join("PL_MAIN.csv").exists());

    // 8. split-genre --min-tracks 2 --execute twice (second run is idempotent!)
    let pl_genre = Playlist {
        id: "PL_GENRE".to_string(),
        title: "GenreMix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 3,
        tracks: vec![
            track(
                "V_ZEB",
                "SG_1",
                0,
                "Zebrahead",
                "Anthem",
                210,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_ATV",
                "SG_2",
                1,
                "Daft Punk",
                "Get Lucky",
                249,
                MusicVideoType::AudioTrack,
            ),
            track(
                "V_ABBA",
                "SG_3",
                2,
                "ABBA",
                "Waterloo",
                168,
                MusicVideoType::AudioTrack,
            ),
        ],
    };
    let genre_client = StatefulMockClient::new(vec![pl_genre], HashMap::new());
    for pass in 0..2 {
        out.clear();
        let mut ctx = RuntimeContext {
            client: &genre_client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::SplitGenre(SplitGenreArgs {
                playlist: "PL_GENRE".to_string(),
                config: None,
                min_tracks: 2,
                execute: true,
            }),
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("split genre");
        if pass == 0 {
            assert_eq!(genre_client.mutations(), 1);
        } else {
            // Second pass sees "GenreMix - Rock" already has both tracks -> 0 new mutations!
            assert_eq!(genre_client.mutations(), 1);
        }
    }
}

#[tokio::test]
async fn e2e_health_upgrade_and_headless_tui_tabs() {
    let dir = TempDir::new().expect("temp dir");
    let paths = AppPaths::from_root(dir.path());
    paths.ensure_dirs().expect("ensure dirs");
    let config = AppConfig::default();

    let mut del = track(
        "V_DEL",
        "SH_1",
        0,
        "Gone",
        "Deleted Track",
        180,
        MusicVideoType::OfficialMusicVideo,
    );
    del.availability = TrackAvailability::Deleted;
    let ugc = track(
        "V_UGC",
        "SH_2",
        1,
        "Uploader",
        "Muse - Hysteria (Official Video)",
        228,
        MusicVideoType::UserGenerated,
    );
    let pl_health = Playlist {
        id: "PL_HEALTH".to_string(),
        title: "HealthMix".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 2,
        tracks: vec![del, ugc],
    };
    let mut catalog = HashMap::new();
    catalog.insert(
        "hysteria".to_string(),
        vec![track(
            "V_ATV_HYST",
            "S_NEW",
            0,
            "Muse",
            "Hysteria",
            227,
            MusicVideoType::AudioTrack,
        )],
    );

    let client = StatefulMockClient::new(vec![pl_health], catalog);
    let store = Arc::new(SqliteStore::open_in_memory(Some(paths.snapshots_dir())).expect("store"));
    let http = MockEnrichmentHttp::with_responses(Vec::new());
    let enricher = EnrichmentService::new(
        Arc::clone(&store),
        http,
        EnrichmentConfig::default(),
        GenreMatcher::built_in_default(),
    );
    let verifier = MockVerifier;

    let mut out = Vec::new();
    {
        let mut ctx = RuntimeContext {
            client: &client,
            store: store.as_ref(),
            enricher: &enricher,
            verifier: &verifier,
            paths: &paths,
            config: &config,
            raw_auth_input: None,
            out: &mut out,
        };
        execute_command(
            &Commands::Health(HealthArgs {
                target: "PL_HEALTH".to_string(),
                find_official: true,
                prune_unavailable: true,
                upgrade: true,
                execute: true,
            }),
            &mut ctx,
            |_| Ok(()),
        )
        .await
        .expect("health upgrade + prune execute");
    }

    let final_pl = client.get_playlist("PL_HEALTH").await.expect("pl");
    assert_eq!(final_pl.tracks.len(), 1);
    assert_eq!(final_pl.tracks[0].video_id, "V_ATV_HYST");

    // Verify `Commands::Tui` and headless 5-tab Ratatui TestBackend rendering
    let mut captured_tab = ActiveTab::Playlists;
    let mut ctx = RuntimeContext {
        client: &client,
        store: store.as_ref(),
        enricher: &enricher,
        verifier: &verifier,
        paths: &paths,
        config: &config,
        raw_auth_input: None,
        out: &mut out,
    };
    execute_command(&Commands::Tui, &mut ctx, |mut app: TuiApp| {
        for expected in ActiveTab::ALL {
            assert_eq!(app.active_tab, expected);
            let buf = app.render_to_test_buffer(100, 30);
            let rendered: String = buf.content().iter().map(|c| c.symbol()).collect();
            assert!(rendered.contains("ytm-architect"));
            assert!(rendered.contains(expected.title()));
            app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        }
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            TuiAction::Quit
        );
        captured_tab = app.active_tab;
        Ok(())
    })
    .await
    .expect("tui command");
    assert_eq!(captured_tab, ActiveTab::Playlists);
}
