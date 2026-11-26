use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::*;

async fn spawn_http_sequence(responses: Vec<&'static str>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind local test server");
    let addr = listener.local_addr().expect("local addr");
    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = Arc::clone(&hits);

    tokio::spawn(async move {
        for raw_resp in responses {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            hits_clone.fetch_add(1, Ordering::SeqCst);
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf).await;
            let _ = stream.write_all(raw_resp.as_bytes()).await;
            let _ = stream.shutdown().await;
        }
    });

    (format!("http://{addr}/"), hits)
}

#[test]
fn jittered_delay_stays_within_plus_minus_25_percent() {
    for attempt in 0..20 {
        let base = 1_000;
        let d = jittered_delay_ms(base, attempt);
        assert!(
            (750..=1_250).contains(&d),
            "jittered delay {d} out of [750, 1250]"
        );
    }
    assert_eq!(jittered_delay_ms(0, 0), 0);
}

#[tokio::test]
async fn api_key_recovers_after_transient_discovery_failure() {
    let (discovery_url, hits) = spawn_http_sequence(vec![
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "HTTP/1.1 200 OK\r\nContent-Length: 35\r\nConnection: close\r\n\r\n{\"INNERTUBE_API_KEY\":\"RECOVERED99\"}",
    ])
    .await;

    let transport =
        ReqwestTransport::with_test_urls(YTM_BASE_API.to_string(), Some(discovery_url), 0)
            .expect("transport");

    // First call fails to fetch key: must NOT cache `None`.
    let first = transport.resolve_params().await;
    assert_eq!(first, YTM_PARAMS_BASE);

    // Second call must retry discovery and cache the recovered key.
    let second = transport.resolve_params().await;
    assert_eq!(second, format!("{YTM_PARAMS_BASE}&key=RECOVERED99"));

    // Third call hits cached key in RwLock (no extra HTTP hit).
    let third = transport.resolve_params().await;
    assert_eq!(third, format!("{YTM_PARAMS_BASE}&key=RECOVERED99"));
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn mutation_safe_policy_does_not_blind_retry_503() {
    let (base_url, hits) = spawn_http_sequence(vec![
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    ])
    .await;

    let transport = ReqwestTransport::with_test_urls(base_url, None, 2).expect("transport");
    let err = transport
        .post_json_with_policy(
            "browse/edit_playlist",
            &json!({}),
            &[],
            RetryPolicy::MutationSafe,
        )
        .await
        .unwrap_err();

    assert!(matches!(err, CoreError::RateLimited { status: 503, .. }));
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}
