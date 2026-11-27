use ytm_store::{EnrichmentCacheStore, SqliteStore};

#[tokio::test]
async fn play_history_insert_and_get() {
    let store = SqliteStore::open_in_memory(None).expect("mem store");
    // Insert a few entries
    store
        .insert_play_history(
            "vid1",
            "Track One",
            "[\"Artist A\"]",
            Some("Album X"),
            Some(200),
        )
        .await
        .expect("insert 1");
    store
        .insert_play_history("vid2", "Track Two", "[\"Artist B\"]", None, Some(180))
        .await
        .expect("insert 2");
    store
        .insert_play_history(
            "vid3",
            "Track Three",
            "[\"Artist C\"]",
            Some("Album Y"),
            Some(220),
        )
        .await
        .expect("insert 3");

    // Get recent history
    let history = store.get_play_history(10).await.expect("get history");
    assert_eq!(history.len(), 3);
    // Most recent first
    assert_eq!(history[0].video_id, "vid3");
    assert_eq!(history[0].title, "Track Three");
    assert_eq!(history[1].video_id, "vid2");
    assert_eq!(history[2].video_id, "vid1");

    // Limit works
    let limited = store.get_play_history(2).await.expect("get limited");
    assert_eq!(limited.len(), 2);
    assert_eq!(limited[0].video_id, "vid3");
    assert_eq!(limited[1].video_id, "vid2");
}

#[tokio::test]
async fn play_history_caps_at_500() {
    let store = SqliteStore::open_in_memory(None).expect("mem store");
    // Insert 510 entries
    for i in 0..510 {
        store
            .insert_play_history(
                &format!("vid{}", i),
                &format!("Track {}", i),
                "[\"Artist\"]",
                None,
                Some(200),
            )
            .await
            .expect("insert");
    }
    let history = store.get_play_history(600).await.expect("get history");
    assert_eq!(history.len(), 500);
    // Most recent should be vid509
    assert_eq!(history[0].video_id, "vid509");
}
