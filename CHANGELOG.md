# Changelog

All notable changes to **Dalga** (`ytm-architect`) are documented in this file.
`.github/workflows/release.yml` extracts the matching `## [vX.Y.Z]` section directly into GitHub Release notes.

## [v0.4.0] — 2026-11-27

### Added
- **All of Artist (`ytm-core`, `ytm-ops`, `ytm-gui`):** Harvests full artist discographies (Albums, Singles, Appears On + continuations + "See all" sub-pages), deduplicates preferring official `ATV` over `UGC`, lets you filter features/compilations in a preview modal, and stages tracks safely into `SyncQueue` with pre-mutation snapshots.
- **Playlist Quick-Add:** Select a favourite target playlist in Settings and stage the currently playing track with one click from the bottom player bar (`allow_duplicates = false`).
- **Audio Power Tools (`audio_fx.rs`):** `mpv` equalizer presets (`Flat`, `Bass Boost`, `Vocal`, custom 5/10-band chains), pitch-corrected playback speed (`0.75x`–`1.5x` via `scaletempo`), and a fade-out Sleep Timer integrated with the Status Ribbon.
- **Play History & Notifications:** Persistent SQLite `play_history` table (capped at 500 rows) + recently played tracking, and opt-in track-change desktop notifications via `notify-send` on Linux (disabled by default).
- **Accessibility:** Configurable `font_scale` (`0.8`–`2.0`), high-contrast WCAG-AA palette toggle (`high_contrast`), and 2px active focus rings.
- **Discovery & Last.fm Scrobble (Opt-In):** "Similar songs" / radio discovery via `WatchNextResult` and an opt-in Last.fm scrobbler (`lastfm.rs`) with 50% / 4-minute threshold and zero network activity when disabled.

### Fixed & Hardened
- **Responsive Top & Player Bars (`bar_layout.rs`, `top_bar.rs`, `player_bar.rs`, `player_bar_right.rs`):** Eliminated horizontal overlap across narrow/tiled windows and `font_scale` zoom levels via `effective_width` breakpoints; centered 3-column Player Bar with prominent `Prev`/`Play-Pause`/`Next` controls and automatic 2-row compact mode below `780px`.
- **Wheel & Touchpad Scrubbing:** Scrolling over the seek bar (`±5s`, `Shift` = `±1s`) or volume control (`±5%`, `Shift` = `±1%`) adjusts playback position or volume smoothly.
- **Apple/Linear Visual Polish & Non-Compounding Font Scale (`theme.rs`, `navigation.rs`):** Fixed compounding `font_scale` in `apply_palette`, refined base typography (`14px` body, `12px` small, `19px` heading) and borderless pill spacing, and raised `Ctrl++` zoom cap to `2.0x` with `560x480` min window size.
- **Dalga Desktop Identity (`lib.rs`, `tray_linux.rs`, `assets/brand/dalga.desktop`, `scripts/install-desktop-linux.sh`):** Switched Wayland `app_id`, Linux tray ID, and `StartupWMClass` from `ytm-architect` to `dalga`, enlarged the top-left logo to `28x28`, and added a Linux desktop/icon installer script.
- Split `>400`-line source/test modules (`player_bar`, `playback_ops`, `worker`, `worker_sync`, `db`, `worker_exec_tests`, `store_integration_test`) to satisfy `CLAUDE.md` size limits, ran `cargo fmt`, and automated per-tag changelog extraction in `.github/workflows/release.yml`.

## [v0.3.0] — 2026-11-27

### Added
- **System Tray & Close-to-Tray:** Theme-aware tray icon (`ksni` on Linux, `tray-icon` on Windows) with Show/Hide, Play/Pause, Next, Previous, Mini Player toggle, and Quit; closing the window hides to tray while audio keeps playing.
- **Borderless Mini Player (`Ctrl+M`):** Always-on-top secondary viewport with Compact & Expanded live search modes, screen-edge snapping, per-monitor position persistence, corner resize, and optional idle opacity dimming.
- **Home Shelves & Continue Listening:** Live personalized Home shelves (`Listen Again`, `Mixed for you`, `New releases`, `Quick picks`), skeleton loaders, a `Continue Listening` card from saved sessions, and offline `browse_cache` hydration.
- **Unified Context Menu & Shortcuts Registry:** Shared `ContextTarget` right-click menu across Home, Search, Browse, Inspector, and Queue, plus a deduplicated `SHORTCUTS` registry displayed in Settings.

## [v0.2.0] — 2026-11-27

### Added
- **Dalga Brand & Vector Icons:** Concept A ribbon-wave SVG/PNG branding, 53 monochrome Lucide SVG icons (`egui_extras`), and zero-emoji UI rendering.
- **Adaptive Theme Engine & Dynamic Accent:** Ultra-Dark & Apple-style Light surfaces following OS theme (`System | Dark | Light`), Emerald/Gold/YTM Pink accents, and off-thread dominant cover accent extraction (`WCAG AA >= 4.5:1`).
- **Live TR/EN i18n & Chrome Redesign:** Instant Turkish/English switching, top-bar Back/Forward + search + Status Ribbon ticker (priority preemption & 50-item history), collapsible Studio Tools sidebar, separate Queue/Lyrics drawer, and Album Artwork Lightbox.
- **Typeahead Search, Session Refresh & Offline Cache:** Debounced InnerTube suggestions (100ms/250ms) with stale-request cancellation, one-click Browser Session Refresh, SQLite `schema v3` offline library/lyrics cache, and `Ctrl+O` / `Ctrl+Shift+O` YouTube Music links.

### Fixed
- Uppercase normalization for InnerTube `gl` region codes (`ClientConfig::normalized_gl`), preventing HTTP 400 `INVALID_ARGUMENT` on localized configs (`gl = "tr"`).
- Fixed sidebar scroll snap-back, `+ Create` playlist UX, `AddTracks` pre-flight deduplication/bisection, and Linux `basictext`-first browser cookie import.

## [v0.1.1] — 2026-11-26

### Fixed
- Fixed `yt-dlp` Brave cookie extraction (`0700` runtime temp dir + `about:blank` + Linux keyring fallbacks) and off-thread Wayland clipboard import (`wl-paste`/`xclip`/`xsel`).

## [v0.1.0] — 2026-11-26

### Added
- Initial 6-crate pure-Rust release (`ytm-core`, `ytm-store`, `ytm-ops`, `ytm-enrich`, `ytm-cli`, `ytm-gui`) with `Configure -> Preview -> Verified POST`, SQLite snapshots, LIS minimal-move sort, fuzzy dedupe, set algebra, health upgrades, and headless `mpv` IPC playback.
