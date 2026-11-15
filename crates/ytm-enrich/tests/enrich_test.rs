use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde_json::json;
use ytm_core::{
    ArtistRef, MusicVideoType, Playlist, PlaylistPrivacy, PlaylistSummary, Track, TrackAvailability,
};
use ytm_enrich::{
    EnrichError, EnrichedTrackMetadata, EnrichmentConfig, EnrichmentService, EnrichmentSource,
    GenreMatcher, MetadataProvider, MockEnrichmentHttp, PacedLimiter, plan_genre_split,
};
use ytm_store::{CachedTrackEnrichment, EnrichmentCacheStore, SqliteStore};

fn sample_track(id: &str, artist: &str, title: &str) -> Track {
    Track {
        video_id: id.to_string(),
        set_video_id: Some(format!("SET_{id}")),
        position: 0,
        title: title.to_string(),
        artists: vec![ArtistRef {
            name: artist.to_string(),
            browse_id: None,
        }],
        album: None,
        duration_sec: Some(210),
        release_year: None,
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
    }
}

#[tokio::test]
async fn cache_hit_makes_zero_http_requests() {
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("sqlite in memory"));
    let row = CachedTrackEnrichment {
        cache_key: "daft punk::around the world".to_string(),
        video_id: Some("VID_DP".to_string()),
        tags: vec!["french house".to_string(), "house".to_string()],
        primary_genre: "Electronic".to_string(),
        release_year: Some(1997),
        bpm: Some(121.0),
        source: "musicbrainz".to_string(),
        updated_at: Utc::now().timestamp(),
    };
    store
        .upsert_track_enrichment(&row)
        .await
        .expect("seed track cache");

    let http = MockEnrichmentHttp::with_responses(Vec::new());
    let cfg = EnrichmentConfig {
        musicbrainz_interval_ms: 0,
        ..Default::default()
    };

    let svc = EnrichmentService::new(store, http, cfg, GenreMatcher::built_in_default());
    let track = sample_track("VID_DP", "Daft Punk", "Around the World");
    let res = svc.enrich_track(&track).await.expect("cache hit");

    assert_eq!(res.source, EnrichmentSource::Cache);
    assert_eq!(res.primary_genre, "Electronic");
    assert_eq!(res.release_year, Some(1997));
    assert_eq!(res.bpm, Some(121.0));
}

#[tokio::test]
async fn cache_miss_queries_musicbrainz_persists_sqlite_and_reuses_cache() {
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("sqlite in memory"));
    let mb_payload = json!({
        "recordings": [{
            "first-release-date": "2001-03-12",
            "tags": [
                { "name": "electronic", "count": 4 },
                { "name": "synthwave", "count": 9 }
            ]
        }]
    });
    let http = MockEnrichmentHttp::with_responses(vec![Ok((200, mb_payload))]);
    let cfg = EnrichmentConfig {
        musicbrainz_interval_ms: 0,
        enable_deezer_fallback: false,
        ..Default::default()
    };

    let svc = EnrichmentService::new(
        Arc::clone(&store),
        http,
        cfg,
        GenreMatcher::built_in_default(),
    );
    let track = sample_track("VID_ONE", "Kavinsky", "Nightcall");

    let first = svc.enrich_track(&track).await.expect("mb fetch");
    assert_eq!(first.source, EnrichmentSource::MusicBrainz);
    assert_eq!(first.primary_genre, "Electronic");
    assert_eq!(first.release_year, Some(2001));

    // Second call uses SQLite even though MockEnrichmentHttp queue is empty.
    let second = svc.enrich_track(&track).await.expect("cache reuse");
    assert_eq!(second.source, EnrichmentSource::Cache);
    assert_eq!(second.primary_genre, "Electronic");

    let cached_artist = store
        .get_artist_genre("kavinsky")
        .await
        .expect("query artist_genres")
        .expect("artist_genres row persisted");
    assert_eq!(cached_artist.primary_genre, "Electronic");
}

#[tokio::test]
async fn deezer_fallback_runs_when_musicbrainz_has_no_tags() {
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("sqlite in memory"));
    let mb_empty = json!({ "recordings": [] });
    let dz_search = json!({
        "data": [{
            "album": { "id": 12345 }
        }]
    });
    let dz_album = json!({
        "release_date": "2018-06-15",
        "genres": {
            "data": [{ "name": "Melodic Death Metal" }]
        }
    });
    let http = MockEnrichmentHttp::with_responses(vec![
        Ok((200, mb_empty)),
        Ok((200, dz_search)),
        Ok((200, dz_album)),
    ]);
    let cfg = EnrichmentConfig {
        musicbrainz_interval_ms: 0,
        ..Default::default()
    };

    let svc = EnrichmentService::new(store, http, cfg, GenreMatcher::built_in_default());
    let track = sample_track("VID_DZ", "Amorphis", "The Bee");
    let res = svc.enrich_track(&track).await.expect("deezer fallback");

    assert_eq!(res.source, EnrichmentSource::Deezer);
    assert_eq!(res.primary_genre, "Metal");
    assert_eq!(res.release_year, Some(2018));
}

#[tokio::test]
async fn paced_limiter_enforces_minimum_interval() {
    let limiter = PacedLimiter::new(Duration::from_millis(80));
    let start = Instant::now();
    limiter.acquire().await;
    limiter.acquire().await;
    assert!(start.elapsed() >= Duration::from_millis(75));
}

#[test]
fn genre_matcher_selects_longest_pattern_first() {
    let toml_cfg = r#"
default_genre = "Other"
min_bucket_tracks = 2

[[rules]]
parent = "Rock"
patterns = ["rock", "wave"]

[[rules]]
parent = "Electronic"
patterns = ["synthwave"]

[[rules]]
parent = "Metal"
patterns = ["metal", "melodic death metal"]
"#;
    let matcher = GenreMatcher::from_toml_str(toml_cfg).expect("valid custom toml");
    assert_eq!(
        matcher.classify(&["rock".to_string(), "melodic death metal".to_string()]),
        "Metal"
    );
    assert_eq!(matcher.classify(&["synthwave".to_string()]), "Electronic");
    assert_eq!(matcher.classify(&["unknown noise".to_string()]), "Other");
}

struct StubProvider(HashMap<String, String>);

#[async_trait::async_trait]
impl MetadataProvider for StubProvider {
    async fn enrich_track(&self, track: &Track) -> Result<EnrichedTrackMetadata, EnrichError> {
        let genre = self
            .0
            .get(&track.video_id)
            .cloned()
            .unwrap_or_else(|| "Other".to_string());
        Ok(EnrichedTrackMetadata {
            cache_key: track.video_id.clone(),
            raw_tags: vec![genre.clone()],
            primary_genre: genre,
            release_year: None,
            bpm: None,
            source: EnrichmentSource::Cache,
        })
    }
}

#[tokio::test]
async fn genre_split_is_idempotent_and_skips_small_buckets() {
    let mut tracks = Vec::new();
    let mut map = HashMap::new();
    for i in 0..3 {
        let id = format!("ROCK_{i}");
        map.insert(id.clone(), "Rock".to_string());
        tracks.push(sample_track(&id, "Artist R", &format!("Rock Song {i}")));
    }
    map.insert("JAZZ_0".to_string(), "Jazz".to_string());
    tracks.push(sample_track("JAZZ_0", "Miles", "Blue"));

    let source = Playlist {
        id: "PL_SRC".to_string(),
        title: "Mixtape".to_string(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: tracks.len(),
        tracks,
    };

    let existing_playlists = vec![PlaylistSummary {
        id: "PL_ROCK".to_string(),
        title: "Mixtape - Rock".to_string(),
        track_count: Some(1),
        owned: true,
        is_liked_music: false,
    }];
    let mut existing_tracks = HashMap::new();
    existing_tracks.insert(
        "PL_ROCK".to_string(),
        vec![sample_track("ROCK_0", "Artist R", "Rock Song 0")],
    );

    let plan = plan_genre_split(
        &source,
        &existing_playlists,
        &existing_tracks,
        &StubProvider(map),
        2,
    )
    .await
    .expect("genre split plan");

    assert_eq!(plan.skipped_small_buckets, 1);
    assert_eq!(plan.buckets.len(), 1);
    let rock = &plan.buckets[0];
    assert_eq!(rock.existing_playlist_id.as_deref(), Some("PL_ROCK"));
    assert_eq!(rock.already_present_count, 1);
    assert_eq!(rock.tracks_to_add.len(), 2);
}
