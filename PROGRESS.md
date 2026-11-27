# PROGRESS.md

Sole authority for dates, release status, and gate verification outputs.

---

## Current Gate Status (2026-11-27 — `v0.3.0` Ready for Tag/Publish)

- **Active Roadmap:** [TASKS.md](TASKS.md) (`v0.2.0` → `v0.5.0`; `v0.3.0` Tasks `13`–`18.1` complete locally; `18.2` tag/CI publish ready upon owner commit/push).
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace --locked`: **PASS** exit 0 (`180/180` tests: `ytm-cli` 6, `ytm-core` 38, `ytm-enrich` 14, `ytm-gui` 93, `ytm-ops` 14, `ytm-store` 15)
- `cargo build --release --locked --bin ytm-gui --bin ytm`: **PASS** exit 0
- `wc -l`: **PASS** 0 `.rs` files > 400 lines (`max = 399` in `crates/ytm-store/tests/store_integration_test.rs` & `crates/ytm-gui/src/views/player_bar.rs`)
- `rg -n "context_menu" crates/ytm-gui/src/views`: **PASS** single shared builder (`context_menu.rs`) adopted across `browse.rs`, `home_cards.rs`, `inspect_rows.rs`, `search.rs`, `queue_lyrics.rs`
- **Latest Published Release:** `v0.2.0` (`c0d9a70`, Linux & Windows CI run `45983802647` **success**; local working tree holds `v0.3.0` implementation and version bump ready for owner commit/tag/push).

---

## 2026-11-27 — `v0.3.0` (Tray, Mini Player, Home Shelves, Unified Context Menu, Shortcuts)

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
