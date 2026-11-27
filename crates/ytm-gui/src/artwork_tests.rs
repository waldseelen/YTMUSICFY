use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use super::*;

#[test]
fn decode_image_bytes_handles_valid_png_and_rejects_garbage() {
    assert!(decode_image_bytes(b"not an image").is_none());

    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([255, 0, 128, 255]));
    let mut buf = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .expect("encode png");
    let decoded = decode_image_bytes(&buf).expect("decode png");
    assert_eq!(decoded.size, [4, 4]);
}

#[test]
fn disk_cache_roundtrip_evicts_oldest_over_limit() {
    let dir = std::env::temp_dir().join(format!("ytm-art-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode png");

    let max_bytes = (png.len() as u64) * 2;
    write_disk_cache(&dir, "https://example.com/1.png", &png, max_bytes);
    std::thread::sleep(Duration::from_millis(15));
    write_disk_cache(&dir, "https://example.com/2.png", &png, max_bytes);
    std::thread::sleep(Duration::from_millis(15));
    write_disk_cache(&dir, "https://example.com/3.png", &png, max_bytes);

    assert!(read_disk_cache(&dir, "https://example.com/1.png", THUMB_MAX_DIM).is_none());
    assert!(read_disk_cache(&dir, "https://example.com/2.png", THUMB_MAX_DIM).is_some());
    assert!(read_disk_cache(&dir, "https://example.com/3.png", THUMB_MAX_DIM).is_some());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn failed_url_ttl_blocks_then_allows_retry() {
    let mut failed = HashMap::new();
    let t0 = Instant::now();
    failed.insert("https://example.com/bad.png".to_string(), t0);

    assert!(is_failure_cached(
        &mut failed,
        "https://example.com/bad.png",
        t0 + Duration::from_secs(30),
        FAILED_RETRY_TTL
    ));
    assert!(!is_failure_cached(
        &mut failed,
        "https://example.com/bad.png",
        t0 + Duration::from_secs(61),
        FAILED_RETRY_TTL
    ));
    assert!(!failed.contains_key("https://example.com/bad.png"));
}

#[test]
fn disk_cache_path_uses_sha256_hex_digest() {
    let dir = Path::new("/tmp/art-cache");
    let p1 = disk_cache_path(dir, "https://lh3.googleusercontent.com/a");
    let p2 = disk_cache_path(dir, "https://lh3.googleusercontent.com/b");
    assert_ne!(p1, p2);
    let name = p1.file_name().and_then(|n| n.to_str()).unwrap();
    assert_eq!(name.len(), 68);
    assert!(name.ends_with(".img"));
    assert!(name[..64].chars().all(|c| c.is_ascii_hexdigit()));
    let expected = format!(
        "{}.img",
        hex::encode(Sha256::digest(b"https://lh3.googleusercontent.com/a"))
    );
    assert_eq!(name, expected);
}
