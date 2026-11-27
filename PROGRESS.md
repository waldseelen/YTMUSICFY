# PROGRESS.md

Sole authority for dates, release status, and gate verification outputs.

---

## Current Gate Status (2026-11-27 — `v0.4.0` Published)

- **Active Roadmap:** [TASKS.md](TASKS.md) (`v0.2.0` → `v0.5.0`; `v0.4.0` Tasks `19`–`25` complete; next phase `v0.5.0`).
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace --locked`: **PASS** exit 0 (`205/205` tests: `ytm-cli` 6, `ytm-core` 42, `ytm-enrich` 14, `ytm-gui` 109, `ytm-ops` 17, `ytm-store` 17)
- `cargo build --release --locked --bin ytm-gui --bin ytm`: **PASS** exit 0 (local release build + Linux desktop/icon installation verified)
- `wc -l`: **PASS** 0 `.rs` files > 400 lines (`max = 399` in `crates/ytm-store/tests/store_integration_test.rs`)
- `rg -n "context_menu" crates/ytm-gui/src/views`: **PASS** single shared builder (`context_menu.rs`) adopted across `browse.rs`, `home_cards.rs`, `inspect_rows.rs`, `search.rs`, `queue_lyrics.rs`
- **Latest Published Release:** `v0.4.0` (Linux & Windows CI run **success**; `SHA256SUMS.txt` published).

---

## 2026-11-27 — `v0.4.0` (All of Artist, Quick-Add, Audio FX, History, Notifications, Accessibility, Last.fm)

- **Verified `SHA256SUMS.txt` (`v0.4.0`, CI run **success**):**
  - `ytm-architect-v0.4.0-linux-x86_64.tar.gz`: `573c75b540d5690b3a972933e894788c55f8775b6f99bbf3cf8f3dacefb81287`
  - `ytm` (Linux): `b08f651f374973af368b2de3bb9b4a2e00fcad1cc7fcdbe7a68d87798b963fed`
  - `ytm-gui` (Linux): `334ebe237b47b0975a441d4423399f93f09ffa858315eaa3445d751c56a924b1`
  - `ytm-architect-v0.4.0-windows-x86_64.zip`: `8eb607b577810ebf24f9054478d7c438103168d8e514220b33ab01ed7937df7f`
  - `ytm.exe` (Windows): `9f6515ccd9b0b11d81b8d0354cfa59867e792748e19dcec1626b03552f3a0b7b`
  - `ytm-gui.exe` (Windows): `7909904d30774611e5f45ed094cf9f4c449b62608cd229d0594721e442dd416b`

- **Task 19 — All of Artist (`ytm-core/src/artist_harvest.rs`, `ytm-ops/src/artist_plan.rs`, `crates/ytm-gui/src/views/artist_harvest.rs`):**
  - Harvest: recursive artist page walk (albums, singles, appears-on shelves + continuations + "See all" sub-pages) with `tests/fixtures/artist_*.json` fixtures; strict ATV-over-UGC dedupe in `ytm-ops::plan_artist_harvest` with `include_features` / `include_compilations` options; unit tests for shelf classification (TR/EN), continuation walking, and option filters.
  - Apply: context-menu `All of Artist` enabled when artist browse ID present; harvest preview modal (`New Playlist` / `Existing Playlist` picker) stages `AddTracks` with `allow_duplicates = false`; mocked-client test asserts mandatory pre-sync SQLite snapshot precedes POST and read-back runs.

- **Task 20 — Playlist Quick-Add:**
  - `favourite_playlist_id` config field + player-bar one-click `+ Favourite Playlist` button staging `AddTracks` (`allow_duplicates = false`); unit test verifies staged op and snapshot/read-back.

- **Task 21 — Audio Power Tools (`crates/ytm-gui/src/audio_fx.rs`, player-bar integration):**
  - EQ presets (Flat, Bass Boost, Vocal, Custom 5/10-band) via mpv `af=equalizer` filter chains; speed 0.75x–1.5x pitch-corrected via `scaletempo`; sleep timer state machine (armed → fading → paused) with injected `Instant` clock and Ribbon status; all with unit tests.

- **Task 22 — History & Notifications:**
  - 22.1 Play history SQLite table (`play_history`, capped at 500 entries, `played_at` index) with insert/get/cap tests; auto-recorded on track start via worker.
  - 22.2 Desktop notifications opt-in (`notifications_enabled` config, default `false`); non-blocking `notify-send -a Dalga -- <summary> <body>` on Linux with disabled-path unit tests (zero calls when off).

- **Task 23 — Accessibility (`crates/ytm-gui/src/theme.rs`):**
  - Font scale (`font_scale`, 0.8–2.0) applied to all text styles; high-contrast palette (`high_contrast` bool) with WCAG-AA-safe colors; visible focus ring (2px accent stroke) on active widgets; AccessKit readiness via existing `Icon` enum tooltip labels.

- **Task 24 — Discovery Extras:**
  - 24.1 Similar & Radio: existing `WatchNextResult` / `LoadWatchNext` infrastructure reused for "Similar songs" shelf and radio station from any track/album/artist.
  - 24.2 Last.fm scrobble opt-in (`crates/ytm-gui/src/lastfm.rs`): injected-clock state machine (idle → pending → submitting → submitted/failed) with 50%/4-min threshold; credentials stored like auth; disabled = zero network; unit tests for threshold calc and state transitions.

- **Task 25 — Release & Post-Release Audit (`v0.4.0`):**
  - Release notes automation: added [CHANGELOG.md](CHANGELOG.md) (`[v0.4.0]`–`[v0.1.0]`) and updated `.github/workflows/release.yml` to extract `## [${TAG}]` into `release-notes.md` (with `git log` fallback) so GitHub Releases include full features, changes, and bug fixes.
  - Module size cleanup (`<= 400` lines): split `views/player_bar_right.rs`, `worker_exec_artist_tests.rs`, `playback_ops_tests.rs`, `ytm-store/src/play_history.rs`, `ytm-store/src/schema.sql`, `ytm-store/tests/play_history_test.rs`, `worker_jobs::spawn_execute_mutation`, and `worker_sync_ops::execute_set_like_status_batch`.
  - Responsive bar redesign & Omarchy/Dalga desktop polish (`bar_layout.rs`, `top_bar.rs`, `player_bar.rs`, `player_bar_right.rs`, `theme.rs`, `navigation.rs`, `lib.rs`, `tray_linux.rs`, `scripts/install-desktop-linux.sh`): `effective_width` breakpoints, 2-row compact Player Bar (`< 780px`), centered 3-column wide Player Bar, wheel/touchpad seek & volume scrubbing, non-compounding Apple/Linear typography & spacing, `with_app_id("dalga")` + `StartupWMClass=dalga` + `ytm-architect.desktop` cleanup.

---

## 2026-11-27 — `v0.3.0` (Tray, Mini Player, Home Shelves, Unified Context Menu, Shortcuts)

- **Verified `SHA256SUMS.txt` (`v0.3.0`, CI run `45994855203`):**
  - `ytm-architect-v0.3.0-linux-x86_64.tar.gz`: `0997d6b66d8a1e7a3d1fced79a43feaa61cbd2abceddfb5c02466953d0a12100`
  - `ytm` (Linux): `fab6cb46d66a016f662f23fe88dcf3cd71cfb97155d41e7973f32b449c080a56`
  - `ytm-gui` (Linux): `2379190af9d0f48e35790c06a3dab0de479e253640124dacfb1a342b8a352b54`
  - `ytm-architect-v0.3.0-windows-x86_64.zip`: `2c9d92ad7374213c1a976e1b6ff24a79249b9ac106de3d8fde6f387df759dfb8`
  - `ytm.exe` (Windows): `4e81395521f5a47e4ac358824bd50763f2574fdeecdca8fbbe059497bb231331`
  - `ytm-gui.exe` (Windows): `11ce0a990c27ade371a7aedc48581f7417f3415c3619830bf11f00306b287d90`

- **Task 13 — Window Lifecycle & System Tray (`tray.rs`, `tray_linux.rs`, `tray_windows.rs`, `window_lifecycle.rs`):**
  - Implemented `TrayBackend` trait with `LinuxKsniTray` (`ksni` StatusNotifierItem, isolated Tokio thread, no GTK), `WindowsTray` (`tray-icon` + `muda` Win32 backend), `NoopTrayBackend` fallback, theme-aware monochrome Dalga icon (`build_tray_rgba`), and tray menu (`Show/Hide`, `Play/Pause`, `Previous`, `Next`, `Mini Player`, `Quit`).
  - Wired `resolve_close_request` close-to-tray reducer and Settings toggles (`tray_enabled`, `close_to_tray`) keeping playback active when hidden.
- **Task 14 — Borderless Mini Player (`mini_player.rs`, `views/mini_player_view.rs`, `views/mini_player_search.rs`):**
  - Implemented always-on-top borderless `egui` secondary viewport (`Dalga Mini`) with Compact and Expanded modes sharing `app.search_controller` (top 3 instant results), monitor clamping, Spotify-style edge snapping, corner resize grip, per-monitor persisted geometry (`mini_player_layouts`), and optional idle opacity (`mini_player_opacity_on_idle`).
- **Task 15 — Home Shelves, Continue Card & Offline Hydration (`parser/browse_cards.rs`, `offline_cache.rs`, `views/home_cards.rs`, `views/browse.rs`):**
  - Enhanced `ytm-core` Home parser (`tests/fixtures/home_browse.json`, `home_browse_test.rs`) using recursive tree-search helpers (`watchPlaylistEndpoint` mixes, `Listen again`, `Mixed for you`, `New releases`, `Quick picks`).
  - Added SQLite `browse_cache` (`schema v4`), stale-while-revalidate Home startup hydration, horizontal shelf scrolling, skeleton loading, and **Continue Listening** card from saved playback session.
- **Task 16 — Unified Context Menu (`views/context_menu_model.rs`, `views/context_menu.rs`):**
  - Unified right-click `ContextTarget` (`Track`, `Album`, `Artist`, `Playlist`) across Inspector, Search, Browse, Home, and Queue (with `All of Artist` disabled until `v0.4.0`).
- **Task 17 — Keyboard-First (`shortcuts.rs`, `interaction.rs`, `views/settings.rs`):**
  - Added `/` search focus, `Ctrl+M` Mini Player toggle, duplicate-checked `SHORTCUTS` registry, and Settings shortcuts table generated directly from `SHORTCUTS`.
- **Task 18.1 — Screenshots, Browse Polish & Docs:**
  - Updated `assets/screenshots/home.png` and captured `assets/screenshots/mini-player.png`; bumped workspace version to `0.3.0`; synced `README.md`, `SPEC.md`, `ARCHITECTURE.md`, and `TASKS.md`.
  - Fixed document-order Browse section collection (`visit_browse_sections`) so Artist Top Songs stay above carousels, added `musicPlaylistShelfRenderer`/`musicPlaylistShelfContinuation` + `PL->VL` normalization for third-party playlists, added `straplineTextOne`/`facepile` clickable header artists + `"See all ->"` (`params`) + `+ Stage All`, filtered Turkish `"dinlenme"` play counts, and fixed Search overlay `playNavigationEndpoint` card detection (`artist_album_playlist_browse_test.rs`, `search_cards_test.rs`).

---

## 2026-11-27 — `v0.2.0` ("Dalga") Release & Post-Release Local Fixes

- **`c0d9a70` (`v0.2.0`, CI run `45983802647`):**
  - Completed `TASKS.md` Tasks `1`–`12`: `dalga-mark.svg` / `dalga-wordmark-{dark,light}.svg` / `dalga-tray-16.{svg,png}` vectors, 53 monochrome Lucide SVG icons (`icons.rs`), Ultra-Dark & Apple-style Light themes + OS live watcher (`theme.rs`, `theme_detect.rs`, `theme_accent.rs`), TR/EN live i18n (`i18n*.rs`), top-bar redesign + Status Ribbon + collapsible Studio Tools + independent Queue/Lyrics drawer + album-art Lightbox, debounced InnerTube instant search (`SearchController` + fixture parser: 100ms suggestions / 250ms live results), one-click Browser Session Refresh, SQLite `schema v3` offline library & lyrics cache, YouTube Music links (`Ctrl+O` / `Ctrl+Shift+O`), `gl` country code uppercase normalization (`ClientConfig::normalized_gl`), 5 headless Hyprland screenshots, and full English `README.md`.
  - Verified `SHA256SUMS.txt` (`v0.2.0` assets):
    - `ytm-architect-v0.2.0-linux-x86_64.tar.gz`: `bcf1a81cb64475115417f96ff63211274ccbae2cb84e7a284636464c3e04c458`
    - `ytm` (Linux): `b6b3c106273c4b029073925dcc2adb6a0e701491808de3a1ae1ec1d515335249`
    - `ytm-gui` (Linux): `44dcc3b04c9a80c54809f563922f2dc1f543b50bc8ba3a3f4b905789ec501f6d`
    - `ytm-architect-v0.2.0-windows-x86_64.zip`: `3fc9d59012e7e06ede8424bf9ec7aa0010a7d48da4899dfdeaa24b80c6eb53a5`
    - `ytm.exe` (Windows): `8385c1a0115c36fbbf36b9f7c1cb83363cb04909d8fe52538d083162fad06758`
    - `ytm-gui.exe` (Windows): `6f6ad025fff8510b05c8f0812c0e8e3604942c27a02e4d3fffb413ca47b51643`
- **Local Uncommitted Hotfixes (Ready for `v0.2.1` upon owner commit/push):**
  - Fixed per-frame `scroll_to_me` snap-back in `sidebar.rs` and `views/inspect_rows.rs`.
  - Updated playlist creation input/button labels (`+ Create` / `+ Olustur`) to remain clickable and auto-open Sync Drawer on submit.
  - Added pre-flight deduplication/unavailable filtering (`collect_add_ids`) and `STATUS_FAILED` tail bisection (`add_tracks_resilient`) in `worker_sync_ops.rs` + `worker_sync.rs`.

---

## 2026-11-27 — Dalga Branding, Roadmap Reset & Wayland/Keyring Fixes

- **`fc69dca` — Dalga Brand & Assets (`TASKS.md` Task `2.2`–`2.4`):**
  - Created `crates/ytm-gui/src/brand.rs` centralizing `Dalga` visible strings and embedded Concept A monochrome 3D logos (`assets/brand/dalga-{dark,light}*.png`, `dalga-icon-256.png`, `dalga.ico`, `dalga.desktop`).
  - Wired window icon + title (`lib.rs`), top bar logo/label (`overlays.rs`), MPRIS display name (`media_controls.rs`), TUI title (`tui_render.rs`), and theme-aware README `<picture>` header (`README.md`).
- **`8520607` & `51d37fb` — Roadmap Reset (`TASKS.md`):**
  - Archived v0.1.x re-architecture plan in git history (`43/43` complete) and wrote `v0.2.0` → `v0.5.0` roadmap with locked decisions (`B1`–`G1`, `N1`, `K1`–`K6`, `M2` owner-approved).
- **`437f119` — Wayland ANR & Keyring Prompt Fixes:**
  - `crates/ytm-gui/src/lib.rs`: disabled blocking GL `vsync` when `WAYLAND_DISPLAY` is set (`should_enable_vsync`) to stop `eglSwapBuffers` deadlocks on hidden Hyprland workspaces.
  - `crates/ytm-gui/src/{app_events,player,player_ipc}.rs`: throttled inactive/minimized `time-pos` & status repaints (`200ms`/`500ms` → `1500ms`).
  - `crates/ytm-gui/src/auth_store.rs`: read `0600` `auth.json` directly and gate `secret-tool` behind `YTM_USE_SECRET_SERVICE=1`, eliminating GNOME Keyring modal prompts on autologin desktops.

---

## 2026-11-26 — Re-Architecture Closure & Releases (`v0.1.0` → `v0.1.1`)

- **`v0.1.1` (`cb859ea`, CI run `45852052661`):** Fixed `yt-dlp` Brave cookie extraction (`0700` `TempDir` + `about:blank` + Linux keyring/basictext fallbacks in `worker_auth.rs`) and added off-thread Wayland clipboard import (`WorkerCmd::ImportClipboardAuth` via `wl-paste`/`xclip`/`xsel`/`powershell`).
- **`v0.1.0` (`1a8bb22` / `8060540`, CI run `45839060451`):** Completed Phases 0–5 (`43/43` tasks) across all 6 crates, fixed Windows `souvlaki` SMTC `HWND` init (`media_controls.rs`), and published initial Linux + Windows binaries with `SHA256SUMS.txt`.
