pub mod mock_client;

use std::sync::Arc;

use anyhow::Result;
use tempfile::TempDir;
use ytm_cli::{AppConfig, Commands, RuntimeContext, TuiApp, execute_command};
use ytm_core::{AlbumRef, ArtistRef, MusicVideoType, Track, TrackAvailability};
use ytm_enrich::{EnrichmentConfig, EnrichmentService, GenreMatcher, MockEnrichmentHttp};
use ytm_store::{AppPaths, SqliteStore};

pub use mock_client::{MockVerifier, StatefulMockClient};

pub fn track(
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
        thumbnail_url: None,
    }
}

pub struct CliHarness {
    pub _dir: TempDir,
    pub paths: AppPaths,
    pub config: AppConfig,
    pub store: Arc<SqliteStore>,
    pub enricher: EnrichmentService<SqliteStore, MockEnrichmentHttp>,
    pub verifier: MockVerifier,
}

impl CliHarness {
    pub fn new(http: MockEnrichmentHttp) -> Self {
        let dir = TempDir::new().expect("temp dir");
        let paths = AppPaths::from_root(dir.path());
        paths.ensure_dirs().expect("ensure dirs");
        let store =
            Arc::new(SqliteStore::open_in_memory(Some(paths.snapshots_dir())).expect("store"));
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
        Self {
            _dir: dir,
            paths,
            config: AppConfig::default(),
            store,
            enricher,
            verifier: MockVerifier,
        }
    }

    pub async fn run_cmd(
        &self,
        client: &StatefulMockClient,
        cmd: &Commands,
        raw_auth_input: Option<String>,
    ) -> Result<String> {
        self.run_cmd_with_tui(client, cmd, raw_auth_input, |_| Ok(()))
            .await
    }

    pub async fn run_cmd_with_tui<F>(
        &self,
        client: &StatefulMockClient,
        cmd: &Commands,
        raw_auth_input: Option<String>,
        tui_runner: F,
    ) -> Result<String>
    where
        F: FnOnce(TuiApp) -> Result<()>,
    {
        let mut out = Vec::new();
        let mut ctx = RuntimeContext {
            client,
            store: self.store.as_ref(),
            enricher: &self.enricher,
            verifier: &self.verifier,
            paths: &self.paths,
            config: &self.config,
            raw_auth_input,
            out: &mut out,
        };
        execute_command(cmd, &mut ctx, tui_runner).await?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }
}
