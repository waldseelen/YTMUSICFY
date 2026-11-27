use std::path::Path;

use sha2::{Digest, Sha256};
use ytm_core::{
    AlbumRef, ArtistRef, BrowseCard, BrowseCardKind, BrowsePage, BrowsePageKind, BrowseSection,
    FetchManifest, MusicVideoType, Playlist, PlaylistPrivacy, PlaylistSummary, SearchFilter,
    SearchPage, SessionStatus, Track, TrackAvailability,
};

use crate::app::YtmGuiApp;
use crate::contracts::{AccentPalette, ThemeMode, UiLang};
use crate::state::{AsyncState, GuiTab};
use crate::sync_queue::StagedOp;

fn write_demo_cover(cache_dir: &Path, url: &str, c1: [u8; 3], c2: [u8; 3]) {
    let hex = hex::encode(Sha256::digest(url.as_bytes()));
    let path = cache_dir.join(format!("{hex}.img"));
    let mut img = image::RgbaImage::new(128, 128);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let t = (x as f32 + y as f32) / 254.0;
        let r = (f32::from(c1[0]) * (1.0 - t) + f32::from(c2[0]) * t) as u8;
        let g = (f32::from(c1[1]) * (1.0 - t) + f32::from(c2[1]) * t) as u8;
        let b = (f32::from(c1[2]) * (1.0 - t) + f32::from(c2[2]) * t) as u8;
        *px = image::Rgba([r, g, b, 255]);
    }
    let mut bytes = Vec::new();
    let _ = image::DynamicImage::ImageRgba8(img).write_to(
        &mut std::io::Cursor::new(&mut bytes),
        image::ImageFormat::Png,
    );
    let _ = std::fs::write(path, bytes);
}

fn demo_track(
    pos: usize,
    vid: &str,
    title: &str,
    artist: &str,
    album: &str,
    dur: u32,
    art: &str,
) -> Track {
    Track {
        video_id: vid.to_string(),
        set_video_id: Some(format!("sid_{pos}")),
        position: pos,
        title: title.to_string(),
        artists: vec![ArtistRef {
            name: artist.to_string(),
            browse_id: Some(format!("UC_{artist}")),
        }],
        album: Some(AlbumRef {
            name: album.to_string(),
            browse_id: Some(format!("MPREb_{album}")),
        }),
        duration_sec: Some(dur),
        release_year: Some(2024),
        music_video_type: MusicVideoType::AudioTrack,
        availability: TrackAvailability::Available,
        thumbnail_url: Some(art.to_string()),
    }
}

/// Populate `YtmGuiApp` with deterministic non-sensitive demo data when `DALGA_PREVIEW_SCENE` is set.
pub(crate) fn apply_preview_scene(app: &mut YtmGuiApp) {
    let Ok(scene) = std::env::var("DALGA_PREVIEW_SCENE") else {
        return;
    };
    if let Some(dirs) = directories::ProjectDirs::from("io", "ytm-architect", "ytm-architect") {
        let art_dir = dirs.cache_dir().join("artwork");
        let _ = std::fs::create_dir_all(&art_dir);
        write_demo_cover(
            &art_dir,
            "https://dalga.local/art/1.png",
            [22, 165, 118],
            [16, 48, 72],
        );
        write_demo_cover(
            &art_dir,
            "https://dalga.local/art/2.png",
            [225, 158, 42],
            [96, 36, 18],
        );
        write_demo_cover(
            &art_dir,
            "https://dalga.local/art/3.png",
            [218, 62, 118],
            [52, 24, 68],
        );
        write_demo_cover(
            &art_dir,
            "https://dalga.local/art/4.png",
            [64, 134, 224],
            [24, 34, 68],
        );
    }

    let tracks = vec![
        demo_track(
            0,
            "d_vid_1",
            "Midnight Harbor",
            "Nora Solis",
            "Coastal Echoes",
            242,
            "https://dalga.local/art/1.png",
        ),
        demo_track(
            1,
            "d_vid_2",
            "Amber Frequency",
            "Kaito Wave",
            "Analog Tides",
            218,
            "https://dalga.local/art/2.png",
        ),
        demo_track(
            2,
            "d_vid_3",
            "Velvet Horizon",
            "Mina Kaya",
            "Nocturne IV",
            265,
            "https://dalga.local/art/3.png",
        ),
        demo_track(
            3,
            "d_vid_4",
            "Glass Currents",
            "Soren Lind",
            "Nordic Drift",
            196,
            "https://dalga.local/art/4.png",
        ),
    ];

    app.session = AsyncState::Ready(SessionStatus {
        authenticated: true,
        account_name: Some("Dalga Studio".to_string()),
        channel_handle: Some("@dalga".to_string()),
        auth_user: "0".to_string(),
        brand_page_id: None,
    });
    app.offline_mode = false;
    app.export_dir = "~/Music/dalga-exports".to_string();
    app.gui_config.ui_lang = UiLang::En;
    app.gui_config.dynamic_accent = false;

    let summaries = vec![
        PlaylistSummary {
            id: "LM".to_string(),
            title: "Liked Music".to_string(),
            track_count: Some(148),
            is_liked_music: true,
            owned: true,
        },
        PlaylistSummary {
            id: "PL_SYNTH".to_string(),
            title: "Late Night Synthwave".to_string(),
            track_count: Some(4),
            is_liked_music: false,
            owned: true,
        },
        PlaylistSummary {
            id: "PL_ACOUSTIC".to_string(),
            title: "Acoustic Morning".to_string(),
            track_count: Some(32),
            is_liked_music: false,
            owned: true,
        },
        PlaylistSummary {
            id: "PL_FOCUS".to_string(),
            title: "Coding Focus".to_string(),
            track_count: Some(64),
            is_liked_music: false,
            owned: true,
        },
    ];
    app.library = AsyncState::Ready(summaries);
    let pl = Playlist {
        id: "PL_SYNTH".to_string(),
        title: "Late Night Synthwave".to_string(),
        description: Some("Curated electronic & ambient essentials".to_string()),
        privacy: PlaylistPrivacy::Private,
        owned: true,
        track_count: tracks.len(),
        tracks: tracks.clone(),
    };
    app.playlist_manifests.insert(
        "PL_SYNTH".to_string(),
        FetchManifest::from_playlist_complete(&pl),
    );
    app.playlist_cache.insert("PL_SYNTH".to_string(), pl);
    app.selected_playlist_id = Some("PL_SYNTH".to_string());
    app.play_queue.start_with(tracks.clone(), 0, 1);
    app.play_queue.set_liked("d_vid_1", true);
    let _ = app.sync_queue.push(StagedOp::AddTracks {
        playlist_id: "PL_SYNTH".to_string(),
        playlist_title: "Late Night Synthwave".to_string(),
        tracks: vec![tracks[2].clone()],
        allow_duplicates: false,
    });
    app.status_bus
        .push_info(true, "Synced 4 playlists cleanly.", app.now_ms());

    let home_cards = vec![
        BrowseCard {
            kind: BrowseCardKind::Album,
            browse_id: Some("MPREb_1".into()),
            video_id: None,
            title: "Coastal Echoes".into(),
            subtitle: "Nora Solis".into(),
            thumbnail_url: Some("https://dalga.local/art/1.png".into()),
        },
        BrowseCard {
            kind: BrowseCardKind::Album,
            browse_id: Some("MPREb_2".into()),
            video_id: None,
            title: "Analog Tides".into(),
            subtitle: "Kaito Wave".into(),
            thumbnail_url: Some("https://dalga.local/art/2.png".into()),
        },
        BrowseCard {
            kind: BrowseCardKind::Artist,
            browse_id: Some("UC_3".into()),
            video_id: None,
            title: "Mina Kaya".into(),
            subtitle: "Artist • 1.2M monthly listeners".into(),
            thumbnail_url: Some("https://dalga.local/art/3.png".into()),
        },
    ];

    let make_home = |cards: Vec<BrowseCard>, qtracks: Vec<Track>| BrowsePage {
        browse_id: "FEmusic_home".into(),
        kind: BrowsePageKind::Home,
        title: "Home".into(),
        subtitle: Some("Personalized mixes & recent albums".into()),
        artists: Vec::new(),
        thumbnail_url: None,
        sections: vec![
            BrowseSection {
                title: "Listen Again".into(),
                cards,
                tracks: Vec::new(),
                browse_id: None,
                browse_params: None,
            },
            BrowseSection {
                title: "Quick Picks".into(),
                cards: Vec::new(),
                tracks: qtracks,
                browse_id: None,
                browse_params: None,
            },
        ],
        tracks: Vec::new(),
        continuation: None,
    };

    match scene.as_str() {
        "home" => {
            app.gui_config.theme_mode = ThemeMode::Dark;
            app.gui_config.accent_palette = AccentPalette::Emerald;
            app.active_tab = GuiTab::Browse;
            app.browse_state = AsyncState::Ready(make_home(home_cards, tracks));
        }
        "search" => {
            app.gui_config.theme_mode = ThemeMode::Dark;
            app.gui_config.accent_palette = AccentPalette::Gold;
            app.active_tab = GuiTab::Search;
            app.catalog_query = "Nora Solis".into();
            app.catalog_filter = SearchFilter::Top;
            let page = SearchPage {
                tracks,
                cards: home_cards,
                continuation: None,
            };
            app.catalog_results = AsyncState::Ready(page.clone());
            app.search_controller.on_query_change("nora", 10);
            app.search_controller.dropdown_open = false;
            let due = app.search_controller.poll_due(300);
            if let Some((rid, _)) = due.fetch_suggestions {
                app.search_controller.apply_suggestions(
                    rid,
                    vec!["nora solis".into(), "nora solis midnight harbor".into()],
                    340,
                );
            }
            if let Some((rid, _)) = due.fetch_live {
                app.search_controller.apply_live_results(rid, page);
            }
            app.search_controller.dropdown_open = true;
        }
        "lightbox" => {
            app.gui_config.theme_mode = ThemeMode::Dark;
            app.gui_config.accent_palette = AccentPalette::YtmPink;
            app.active_tab = GuiTab::Inspector;
            app.lightbox.open(
                "Midnight Harbor",
                "Nora Solis — Coastal Echoes",
                "https://dalga.local/art/1.png",
            );
        }
        "light" => {
            app.gui_config.theme_mode = ThemeMode::Light;
            app.gui_config.accent_palette = AccentPalette::Emerald;
            app.active_tab = GuiTab::Inspector;
            app.right_drawer = crate::right_drawer::RightDrawerTab::Lyrics;
            app.lyrics_video_id = Some("d_vid_1".into());
            app.lyrics_payload = AsyncState::Ready(Some(ytm_core::LyricsPayload::Plain(
                "Lights drift across the bay\nStatic fades before the dawn\nEvery signal finds its wave".into(),
            )));
        }
        "mini" => {
            app.gui_config.theme_mode = ThemeMode::Dark;
            app.gui_config.accent_palette = AccentPalette::Emerald;
            app.active_tab = GuiTab::Browse;
            app.mini_player.open = true;
            app.mini_player.mode = crate::contracts::MiniPlayerMode::Expanded;
            let page = SearchPage {
                tracks: tracks.clone(),
                cards: home_cards.clone(),
                continuation: None,
            };
            app.search_controller.on_query_change("nora", 10);
            app.search_controller.dropdown_open = false;
            let due = app.search_controller.poll_due(300);
            if let Some((rid, _)) = due.fetch_live {
                app.search_controller.apply_live_results(rid, page);
            }
            app.browse_state = AsyncState::Ready(make_home(home_cards, tracks));
        }
        _ => {
            app.gui_config.theme_mode = ThemeMode::Dark;
            app.gui_config.accent_palette = AccentPalette::Emerald;
            app.active_tab = GuiTab::Inspector;
        }
    }
}

/// Capture the current viewport framebuffer to `DALGA_SCREENSHOT_OUT` after warmup frames and return `true` when done.
pub(crate) fn maybe_capture_screenshot(
    ctx: &eframe::egui::Context,
    root_ctx: &eframe::egui::Context,
) -> bool {
    let Ok(out_path) = std::env::var("DALGA_SCREENSHOT_OUT") else {
        return false;
    };
    let req_id = eframe::egui::Id::new("dalga_ss_req");
    let frame_id = eframe::egui::Id::new("dalga_ss_frames");
    let frames: u32 = ctx.data(|d| d.get_temp(frame_id)).unwrap_or(0) + 1;
    ctx.data_mut(|d| d.insert_temp(frame_id, frames));

    let shot = ctx.input(|i| {
        i.events.iter().find_map(|ev| match ev {
            eframe::egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(img) = shot {
        let [w, h] = img.size;
        let rgba: Vec<u8> = img
            .pixels
            .iter()
            .flat_map(|c| [c.r(), c.g(), c.b(), c.a()])
            .collect();
        if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, rgba) {
            let _ = image::DynamicImage::ImageRgba8(buf).save(&out_path);
        }
        root_ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Close);
        return true;
    }

    let already_req: bool = ctx.data(|d| d.get_temp(req_id)).unwrap_or(false);
    if frames >= 4 && !already_req {
        ctx.data_mut(|d| d.insert_temp(req_id, true));
        ctx.send_viewport_cmd(eframe::egui::ViewportCommand::Screenshot(Default::default()));
    }
    ctx.request_repaint();
    false
}
