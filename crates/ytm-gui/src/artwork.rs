use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime};

use directories::ProjectDirs;
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::imageops::FilterType;
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, mpsc as tokio_mpsc};

const MAX_CACHED_TEXTURES: usize = 160;
const MAX_IN_FLIGHT: usize = 48;
const MAX_CONCURRENT_DOWNLOADS: usize = 6;
const MAX_DISK_CACHE_BYTES: u64 = 50 * 1024 * 1024;
const FAILED_RETRY_TTL: Duration = Duration::from_secs(60);

enum ArtMsg {
    Ready { url: String, image: ColorImage },
    Failed { url: String },
}

/// Asynchronous HTTP thumbnail fetcher + bounded disk & GPU texture LRU cache.
pub struct ArtworkCache {
    req_tx: tokio_mpsc::UnboundedSender<String>,
    res_rx: Receiver<ArtMsg>,
    textures: HashMap<String, TextureHandle>,
    lru: VecDeque<String>,
    in_flight: HashSet<String>,
    failed: HashMap<String, Instant>,
}

impl ArtworkCache {
    #[must_use]
    pub fn new(ctx: Option<egui::Context>) -> Self {
        let cache_dir = resolve_artwork_cache_dir();
        let (req_tx, mut req_rx) = tokio_mpsc::unbounded_channel::<String>();
        let (res_tx, res_rx) = mpsc::channel::<ArtMsg>();

        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(async move {
                let Ok(client) = reqwest::Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                else {
                    return;
                };
                let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
                while let Some(url) = req_rx.recv().await {
                    let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                        break;
                    };
                    let client = client.clone();
                    let res_tx = res_tx.clone();
                    let ctx = ctx.clone();
                    let cache_dir = cache_dir.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let msg = match fetch_cached_or_remote(&client, cache_dir, &url).await {
                            Some(image) => ArtMsg::Ready { url, image },
                            None => ArtMsg::Failed { url },
                        };
                        if res_tx.send(msg).is_ok()
                            && let Some(c) = &ctx
                        {
                            c.request_repaint();
                        }
                    });
                }
            });
        });

        Self {
            req_tx,
            res_rx,
            textures: HashMap::new(),
            lru: VecDeque::new(),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
        }
    }

    /// Drain newly decoded images into GPU textures without blocking.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.res_rx.try_recv() {
            match msg {
                ArtMsg::Ready { url, image } => {
                    self.in_flight.remove(&url);
                    self.failed.remove(&url);
                    let tex = ctx.load_texture(url.clone(), image, TextureOptions::LINEAR);
                    self.insert_lru(url, tex);
                }
                ArtMsg::Failed { url } => {
                    self.in_flight.remove(&url);
                    self.failed.insert(url, Instant::now());
                }
            }
        }
    }

    /// Return cached texture for `url` or queue async download if unseen or past retry TTL.
    pub fn texture_for(&mut self, ctx: &egui::Context, url: &str) -> Option<&TextureHandle> {
        self.poll(ctx);
        if self.textures.contains_key(url) {
            self.touch_lru(url);
            return self.textures.get(url);
        }
        if !self.in_flight.contains(url)
            && !is_failure_cached(&mut self.failed, url, Instant::now(), FAILED_RETRY_TTL)
            && self.in_flight.len() < MAX_IN_FLIGHT
        {
            self.in_flight.insert(url.to_string());
            let _ = self.req_tx.send(url.to_string());
        }
        None
    }

    fn insert_lru(&mut self, url: String, tex: TextureHandle) {
        if !self.textures.contains_key(&url) {
            while self.lru.len() >= MAX_CACHED_TEXTURES {
                if let Some(oldest) = self.lru.pop_front() {
                    self.textures.remove(&oldest);
                }
            }
            self.lru.push_back(url.clone());
        } else {
            self.touch_lru(&url);
        }
        self.textures.insert(url, tex);
    }

    fn touch_lru(&mut self, url: &str) {
        if let Some(pos) = self.lru.iter().position(|k| k == url) {
            let item = self.lru.remove(pos).expect("valid idx");
            self.lru.push_back(item);
        }
    }
}

fn is_failure_cached(
    failed: &mut HashMap<String, Instant>,
    url: &str,
    now: Instant,
    ttl: Duration,
) -> bool {
    if let Some(&failed_at) = failed.get(url) {
        if now.saturating_duration_since(failed_at) < ttl {
            return true;
        }
        failed.remove(url);
    }
    false
}

fn resolve_artwork_cache_dir() -> Option<PathBuf> {
    let base = ProjectDirs::from("io", "ytm-architect", "ytm-architect")
        .map(|dirs| dirs.cache_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("ytm-architect-cache"));
    let dir = base.join("artwork");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn disk_cache_path(cache_dir: &Path, url: &str) -> PathBuf {
    let hex = hex::encode(Sha256::digest(url.as_bytes()));
    cache_dir.join(format!("{hex}.img"))
}

fn read_disk_cache(cache_dir: &Path, url: &str) -> Option<ColorImage> {
    let path = disk_cache_path(cache_dir, url);
    let bytes = fs::read(&path).ok()?;
    let image = decode_image_bytes(&bytes)?;
    if let Ok(file) = fs::OpenOptions::new().write(true).open(&path) {
        let _ = file.set_modified(SystemTime::now());
    }
    Some(image)
}

fn write_disk_cache(cache_dir: &Path, url: &str, bytes: &[u8], max_bytes: u64) {
    if fs::create_dir_all(cache_dir).is_err() {
        return;
    }
    let path = disk_cache_path(cache_dir, url);
    let tmp = cache_dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("art")
    ));
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, &path);
        evict_disk_lru(cache_dir, max_bytes);
    }
}

fn evict_disk_lru(cache_dir: &Path, max_bytes: u64) {
    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, SystemTime)> = Vec::new();
    let mut total = 0u64;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            let len = meta.len();
            let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            total = total.saturating_add(len);
            files.push((path, len, modified));
        }
    }
    if total <= max_bytes {
        return;
    }
    files.sort_by_key(|(_, _, mtime)| *mtime);
    for (path, len, _) in files {
        if total <= max_bytes {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(len);
        }
    }
}

async fn fetch_cached_or_remote(
    client: &reqwest::Client,
    cache_dir: Option<PathBuf>,
    url: &str,
) -> Option<ColorImage> {
    if let Some(dir) = cache_dir.clone() {
        let u = url.to_string();
        if let Ok(Some(img)) = tokio::task::spawn_blocking(move || read_disk_cache(&dir, &u)).await
        {
            return Some(img);
        }
    }
    let resp = client
        .get(url)
        .header(reqwest::header::ACCEPT, "image/jpeg,image/png,*/*;q=0.8")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let image = decode_image_bytes(&bytes)?;
    if let Some(dir) = cache_dir {
        let u = url.to_string();
        let raw = bytes.to_vec();
        let _ = tokio::task::spawn_blocking(move || {
            write_disk_cache(&dir, &u, &raw, MAX_DISK_CACHE_BYTES);
        })
        .await;
    }
    Some(image)
}

pub(crate) fn decode_image_bytes(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let resized = if img.width() > 192 || img.height() > 192 {
        img.resize(192, 192, FilterType::Triangle)
    } else {
        img
    };
    let rgba = resized.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

#[cfg(test)]
mod tests {
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

        assert!(read_disk_cache(&dir, "https://example.com/1.png").is_none());
        assert!(read_disk_cache(&dir, "https://example.com/2.png").is_some());
        assert!(read_disk_cache(&dir, "https://example.com/3.png").is_some());
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
}
