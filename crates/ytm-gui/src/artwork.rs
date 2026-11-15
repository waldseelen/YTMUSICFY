use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::imageops::FilterType;

const MAX_CACHED_TEXTURES: usize = 160;
const MAX_IN_FLIGHT: usize = 48;

enum ArtMsg {
    Ready { url: String, image: ColorImage },
    Failed { url: String },
}

/// Asynchronous HTTP thumbnail fetcher + bounded LRU egui texture cache.
pub struct ArtworkCache {
    req_tx: Sender<String>,
    res_rx: Receiver<ArtMsg>,
    textures: HashMap<String, TextureHandle>,
    lru: VecDeque<String>,
    in_flight: HashSet<String>,
    failed: HashSet<String>,
}

impl ArtworkCache {
    #[must_use]
    pub fn new(ctx: Option<egui::Context>) -> Self {
        let (req_tx, req_rx) = mpsc::channel::<String>();
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
                    .timeout(std::time::Duration::from_secs(10))
                    .build()
                else {
                    return;
                };
                while let Ok(url) = req_rx.recv() {
                    let msg = match fetch_and_decode(&client, &url).await {
                        Some(image) => ArtMsg::Ready { url, image },
                        None => ArtMsg::Failed { url },
                    };
                    if res_tx.send(msg).is_err() {
                        break;
                    }
                    if let Some(c) = &ctx {
                        c.request_repaint();
                    }
                }
            });
        });

        Self {
            req_tx,
            res_rx,
            textures: HashMap::new(),
            lru: VecDeque::new(),
            in_flight: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    /// Drain newly decoded images into GPU textures without blocking.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.res_rx.try_recv() {
            match msg {
                ArtMsg::Ready { url, image } => {
                    self.in_flight.remove(&url);
                    let tex = ctx.load_texture(url.clone(), image, TextureOptions::LINEAR);
                    self.insert_lru(url, tex);
                }
                ArtMsg::Failed { url } => {
                    self.in_flight.remove(&url);
                    self.failed.insert(url);
                }
            }
        }
    }

    /// Return cached texture for `url` or queue async download if unseen.
    pub fn texture_for(&mut self, ctx: &egui::Context, url: &str) -> Option<&TextureHandle> {
        self.poll(ctx);
        if self.textures.contains_key(url) {
            self.touch_lru(url);
            return self.textures.get(url);
        }
        if !self.in_flight.contains(url)
            && !self.failed.contains(url)
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

async fn fetch_and_decode(client: &reqwest::Client, url: &str) -> Option<ColorImage> {
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
    decode_image_bytes(&bytes)
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
}
