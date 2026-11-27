use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use ytm_core::{
    AlbumRef, ArtistRef, MusicVideoType, Playlist, PlaylistPrivacy, Track, TrackAvailability,
};
use ytm_enrich::{
    EnrichmentConfig, EnrichmentService, EnrichmentSource, GenreMatcher, MetadataProvider,
    MockEnrichmentHttp, plan_genre_split_with_progress,
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
        thumbnail_url: None,
    }
}

#[test]
fn built_in_matcher_classifies_cyberpunk_soundtrack_and_rnb() {
    let matcher = GenreMatcher::built_in_default();
    assert_eq!(matcher.classify(&["cyberpunk".to_string()]), "Electronic");
    assert_eq!(matcher.classify(&["darksynth".to_string()]), "Electronic");
    assert_eq!(
        matcher.classify(&["video game music".to_string()]),
        "Soundtrack"
    );
    assert_eq!(matcher.classify(&["r&b".to_string()]), "R&B");
}

#[tokio::test]
async fn empty_tag_cache_retries_after_one_day_and_uses_album_hint() {
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("mem store"));
    store
        .upsert_track_enrichment(&CachedTrackEnrichment {
            cache_key: "perturbator::future club".to_string(),
            video_id: Some("VID_PERT".to_string()),
            tags: Vec::new(),
            primary_genre: "Other".to_string(),
            release_year: None,
            bpm: None,
            source: "musicbrainz".to_string(),
            updated_at: Utc::now().timestamp() - 90_000,
        })
        .await
        .expect("seed negative cache");

    let http = MockEnrichmentHttp::with_responses(vec![
        Ok((200, json!({ "recordings": [] }))),
        Ok((200, json!({ "data": [] }))),
        Ok((200, json!({ "data": [{ "album": { "id": 777 } }] }))),
        Ok((
            200,
            json!({
                "release_date": "2014-05-01",
                "genres": { "data": [{ "name": "Darksynth" }] }
            }),
        )),
    ]);
    let cfg = EnrichmentConfig {
        musicbrainz_interval_ms: 0,
        ..Default::default()
    };
    let svc = EnrichmentService::new(store, http, cfg, GenreMatcher::built_in_default());
    let mut track = sample_track("VID_PERT", "Perturbator", "Future Club");
    track.album = Some(AlbumRef {
        name: "Dangerous Days".to_string(),
        browse_id: None,
    });

    let res = svc.enrich_track(&track).await.expect("recovered genre");
    assert_eq!(res.source, EnrichmentSource::Deezer);
    assert_eq!(res.primary_genre, "Electronic");
    assert_eq!(res.release_year, Some(2014));
}

#[tokio::test]
async fn genre_split_progress_records_track_details_skipped_buckets_and_cancels() {
    let store = Arc::new(SqliteStore::open_in_memory(None).expect("mem store"));
    store
        .upsert_track_enrichment(&CachedTrackEnrichment {
            cache_key: "kavinsky::nightcall".to_string(),
            video_id: Some("V1".into()),
            tags: vec!["synthwave".into()],
            primary_genre: "Electronic".into(),
            release_year: Some(2010),
            bpm: None,
            source: "musicbrainz".into(),
            updated_at: Utc::now().timestamp(),
        })
        .await
        .expect("seed V1");

    let svc = EnrichmentService::new(
        store,
        MockEnrichmentHttp::with_responses(vec![]),
        EnrichmentConfig::default(),
        GenreMatcher::built_in_default(),
    );
    let pl = Playlist {
        id: "PL".into(),
        title: "Drive".into(),
        description: None,
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: 1,
        tracks: vec![sample_track("V1", "Kavinsky", "Nightcall")],
    };

    let plan = plan_genre_split_with_progress(&pl, &[], &HashMap::new(), &svc, 2, |_, _, _| {
        ControlFlow::Continue(())
    })
    .await
    .expect("plan ok");
    assert_eq!(plan.buckets.len(), 0);
    assert_eq!(plan.skipped_buckets.len(), 1);
    assert_eq!(plan.track_details["V1"].raw_tags, vec!["synthwave"]);

    let cancelled =
        plan_genre_split_with_progress(&pl, &[], &HashMap::new(), &svc, 1, |_, _, _| {
            ControlFlow::Break(())
        })
        .await;
    assert!(cancelled.is_err());
}
