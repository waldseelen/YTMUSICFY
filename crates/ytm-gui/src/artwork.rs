use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant, SystemTime};

use directories::ProjectDirs;
use eframe::egui::{self, Color32, ColorImage, TextureHandle, TextureOptions};
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, mpsc as tokio_mpsc};

use crate::artwork_hires::{decode_image_with_max_dim, upgrade_artwork_url_hires};

const MAX_CACHED_TEXTURES: usize = 160;
const MAX_IN_FLIGHT: usize = 48;
const MAX_CONCURRENT_DOWNLOADS: usize = 6;
const MAX_DISK_CACHE_BYTES: u64 = 50 * 1024 * 1024;
const FAILED_RETRY_TTL: Duration = Duration::from_secs(60);
const THUMB_MAX_DIM: u32 = 256;
const HIRES_MAX_DIM: u32 = 800;

enum ArtMsg {
    Ready {
        url: String,
        image: ColorImage,
        accent: Option<Color32>,
    },
    Failed {
        url: String,
    },
}

/// Asynchronous HTTP thumbnail fetcher + bounded disk & GPU texture LRU cache.
pub struct ArtworkCache {
    req_tx: tokio_mpsc::UnboundedSender<(String, u32)>,
    res_rx: Receiver<ArtMsg>,
    textures: HashMap<String, TextureHandle>,
    accents: HashMap<String, Option<Color32>>,
    lru: VecDeque<String>,
    in_flight: HashSet<String>,
    failed: HashMap<String, Instant>,
}

impl ArtworkCache {
    #[must_use]
    pub fn new(ctx: Option<egui::Context>) -> Self {
        let cache_dir = resolve_artwork_cache_dir();
        let (req_tx, mut req_rx) = tokio_mpsc::unbounded_channel::<(String, u32)>();
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
                while let Some((url, max_dim)) = req_rx.recv().await {
                    let Ok(permit) = Arc::clone(&semaphore).acquire_owned().await else {
                        break;
                    };
                    let client = client.clone();
                    let res_tx = res_tx.clone();
                    let ctx = ctx.clone();
                    let cache_dir = cache_dir.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let msg =
                            match fetch_cached_or_remote(&client, cache_dir, &url, max_dim).await {
                                Some((image, accent)) => ArtMsg::Ready { url, image, accent },
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
            accents: HashMap::new(),
            lru: VecDeque::new(),
            in_flight: HashSet::new(),
            failed: HashMap::new(),
        }
    }

    /// Drain newly decoded images into GPU textures without blocking.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.res_rx.try_recv() {
            match msg {
                ArtMsg::Ready { url, image, accent } => {
                    self.in_flight.remove(&url);
                    self.failed.remove(&url);
                    let tex = ctx.load_texture(url.clone(), image, TextureOptions::LINEAR);
                    self.accents.insert(url.clone(), accent);
                    self.insert_lru(url, tex);
                }
                ArtMsg::Failed { url } => {
                    self.in_flight.remove(&url);
                    self.failed.insert(url, Instant::now());
                }
            }
        }
    }

    /// Return the precomputed off-thread dominant accent color for `url`, if available.
    #[must_use]
    pub fn accent_for(&self, url: &str) -> Option<Color32> {
        self.accents.get(url).copied().flatten()
    }

    /// Return cached texture for `url` or queue async download if unseen or past retry TTL.
    pub fn texture_for(&mut self, ctx: &egui::Context, url: &str) -> Option<&TextureHandle> {
        self.texture_for_dim(ctx, url, THUMB_MAX_DIM)
    }

    /// Return high-resolution texture (`<= 800px`) for Lightbox / hero headers, falling back to cached thumbnail while loading.
    pub fn texture_for_hires(&mut self, ctx: &egui::Context, url: &str) -> Option<&TextureHandle> {
        let hires = upgrade_artwork_url_hires(url);
        self.poll(ctx);
        if self.textures.contains_key(&hires) {
            self.touch_lru(&hires);
            return self.textures.get(&hires);
        }
        self.queue_fetch(&hires, HIRES_MAX_DIM);
        if self.textures.contains_key(url) {
            self.touch_lru(url);
            return self.textures.get(url);
        }
        None
    }

    fn texture_for_dim(
        &mut self,
        ctx: &egui::Context,
        url: &str,
        max_dim: u32,
    ) -> Option<&TextureHandle> {
        self.poll(ctx);
        if self.textures.contains_key(url) {
            self.touch_lru(url);
            return self.textures.get(url);
        }
        self.queue_fetch(url, max_dim);
        None
    }

    fn queue_fetch(&mut self, url: &str, max_dim: u32) {
        if !self.in_flight.contains(url)
            && !is_failure_cached(&mut self.failed, url, Instant::now(), FAILED_RETRY_TTL)
            && self.in_flight.len() < MAX_IN_FLIGHT
        {
            self.in_flight.insert(url.to_string());
            let _ = self.req_tx.send((url.to_string(), max_dim));
        }
    }

    fn insert_lru(&mut self, url: String, tex: TextureHandle) {
        if !self.textures.contains_key(&url) {
            while self.lru.len() >= MAX_CACHED_TEXTURES {
                if let Some(oldest) = self.lru.pop_front() {
                    self.textures.remove(&oldest);
                    self.accents.remove(&oldest);
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

fn read_disk_cache(
    cache_dir: &Path,
    url: &str,
    max_dim: u32,
) -> Option<(ColorImage, Option<Color32>)> {
    let path = disk_cache_path(cache_dir, url);
    let bytes = fs::read(&path).ok()?;
    let decoded = decode_image_with_max_dim(&bytes, max_dim)?;
    if let Ok(file) = fs::OpenOptions::new().write(true).open(&path) {
        let _ = file.set_modified(SystemTime::now());
    }
    Some(decoded)
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
    max_dim: u32,
) -> Option<(ColorImage, Option<Color32>)> {
    if let Some(dir) = cache_dir.clone() {
        let u = url.to_string();
        if let Ok(Some(img)) =
            tokio::task::spawn_blocking(move || read_disk_cache(&dir, &u, max_dim)).await
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
    let decoded = decode_image_with_max_dim(&bytes, max_dim)?;
    if let Some(dir) = cache_dir {
        let u = url.to_string();
        let raw = bytes.to_vec();
        let _ = tokio::task::spawn_blocking(move || {
            write_disk_cache(&dir, &u, &raw, MAX_DISK_CACHE_BYTES);
        })
        .await;
    }
    Some(decoded)
}

#[cfg(test)]
pub(crate) fn decode_image_bytes(bytes: &[u8]) -> Option<ColorImage> {
    decode_image_with_max_dim(bytes, THUMB_MAX_DIM).map(|(img, _)| img)
}

#[cfg(test)]
#[path = "artwork_tests.rs"]
mod tests;
