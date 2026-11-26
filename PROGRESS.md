# PROGRESS.md

Sole authority for dates, verification gate statuses, and operational session history.

---

## Current Gate Status (2026-11-26 — 100% Re-Architecture Complete)

- `Phase 0 (Contract, Spec & Domain Model Lock)`: **PASS** (5/5 tasks)
- `Phase 1 (Mutation Governance, Strict Staging & Configure -> Preview -> POST)`: **PASS** (8/8 tasks)
- `Phase 2 (Fetch Completeness, Pagination Proof & POST Verification)`: **PASS** (7/7 tasks)
- `Phase 3 (Concurrency, 60 FPS Frame-Rate & Structural Architecture)`: **PASS** (8/8 tasks)
- `Phase 4 (Daily-Driver YouTube Music Replacement)`: **PASS** (7/7 tasks)
- `Phase 5 (Information Architecture, UX Flow & Visual Polish)`: **PASS** (8/8 tasks)
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace`: **PASS** exit 0 (`126/126` tests passed: `ytm-cli` 6, `ytm-core` 31, `ytm-enrich` 14, `ytm-gui` 49, `ytm-ops` 14, `ytm-store` 12)
- `cargo build --release --bin ytm-gui --bin ytm`: **PASS** exit 0 (`Finished release profile [optimized] target(s) in 6.58s`)
- `GitHub Release v0.1.0 (Linux & Windows)`: **PUBLISHED** (`ubuntu-latest` + `windows-latest` CI run `45839060451`, `SHA256SUMS.txt` verified in `README.md`)
- `wc -l`: **PASS** 0 files > 400 LOC across all `.rs` files (`max = 400` in `crates/ytm-gui/src/worker.rs`)

---

## 2026-11-26 — Auth Browser Import (`yt-dlp`) & Wayland Clipboard Freeze Fix

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-gui/src/worker_auth.rs`: `import_browser_auth_inner` now uses a `0700` `TempDir` with a non-existent `cookies.txt` path (`about:blank` + `--no-warnings`), eliminating `yt-dlp`'s `does not look like a Netscape format cookies file` crash on pre-created empty files and adding `+kwallet6`/`+kwallet5`/`+basictext` fallbacks; added `import_clipboard_auth_inner` using async `wl-paste`/`xclip`/`xsel`/`powershell` off the UI thread.
  - `crates/ytm-gui/src/worker.rs` & `worker_msgs.rs`: wired `WorkerCmd::ImportClipboardAuth` and extracted `finish_auth_import` (`400` LOC).
  - `crates/ytm-gui/src/views/auth.rs` & `auth_store.rs`: added **"Import from Clipboard (Wayland-safe)"** button, updated DevTools filter hint (`youtubei/v1` + Home/F5), and dropped obsolete `browser_cookie_temp_path`.
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (`126/126` PASS), `cargo build --release --bin ytm-gui --bin ytm` (`6.58s`), `wc -l <= 400` (max `400`).

---

## 2026-11-26 — Phase 5 Final Polish, Release Build & TASKS.md Closure

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-gui/src/views/player_bar.rs`: 3-zone responsive bottom player bar (`icons.rs` monochrome glyphs, live `mpv` codec/bitrate badge, Mute toggle, `+ Stage Add to Playlist` menu, seek bar hover timestamp tooltip)
  - `crates/ytm-gui/src/playback_ops.rs` & `play_queue.rs`: `PlayQueue::peek_next`, 15-second pre-EOF `player.prefetch_next(&next.video_id)`, `WorkerCmd::SavePlaybackSession` persistence, `poll_os_media_bridge`, `toggle_mute_state`
  - `crates/ytm-gui/src/worker.rs` & `worker_jobs.rs`: split Lane 2 cancellable background planning jobs (`PlanDedupe`, `PlanSort`, `PlanSetOp`, `ScanHealth`, `PlanGenreSplit`) to keep all files `<= 400` LOC
  - `crates/ytm-gui/src/app.rs` & `app_events.rs`: extracted `finish_frame` (`<= 391` LOC), wired `FocusPane` ownership and zero-warning `theme.rs` / `tab_bar.rs` / `command_palette.rs`
  - `TASKS.md`: marked all 43 tasks across Phases 0–5 `[x]`
- **Evidence**:
  - All 5 release gates (`fmt`, `check`, `clippy -D warnings`, `test` 125/125, `build --release`) + `wc -l <= 400` (max 399) passed with exit 0 and 0 warnings.

---

## Previous Checkpoint (2026-11-26 — Phases 1–4 Complete + Phase 5 Core Wired)

- **Touched areas (`path›symbol`)**:
  - **Phase 1 (Governance & Strict Staging):** `playlist_meta_modal.rs`, `sidebar.rs`, `overlays.rs`, `views/repertoire_batch.rs`, `playback_ops.rs`, `worker_sync.rs`, `worker_sync_ops.rs`, `worker_exec.rs`, `worker_auth.rs`, `views/components.rs`, `views/sync.rs`, `views/{dedupe,sort,set_ops,health,genres_snapshots}.rs` (StrictLock default + QuickLike toggle, `PlanFingerprint` staleness, `PlaylistAccess::EditableOwned` & `FetchCompleteness` Tiered Guard, selective sync pruning, atomic `MoveTracks`, position-preserving verified `UpgradeTracks`, SQLite `staged_ops` persistence, opt-in browser cookie refresh)
  - **Phase 3 (Concurrency & 60 FPS):** `worker.rs` (3-lane `tokio::sync::mpsc`: parallel Interactive, cancellable BackgroundJob with `JobId`/`AtomicBool`, serial MutationSerial), `draft_cache.rs` (revision-keyed working-copy cache), `views/repertoire_tree.rs` (`show_rows` virtualization), `views/split_diff.rs` (memoized diff), `artwork.rs` (6-way semaphore + 50 MiB disk LRU + 60s failed TTL), `player_ipc.rs` (Unix + Windows Named Pipe, 200ms throttle, codec/bitrate, `prefetch_next`, `PlaybackErrorKind`), `views/inspect.rs` split into `inspect.rs` + `inspect_toolbar.rs` + `inspect_helpers.rs` + `inspect_rows.rs`
  - **Phase 4 (Daily Driver):** `media_controls.rs` (`souvlaki` MPRIS2/SMTC bridge), `views/search.rs` (6-tab catalog search + continuation + `+ Stage Add`), `views/browse.rs` (Home/Explore/History/Library Albums + Load More + `+ Stage Add`), `views/queue_lyrics.rs` (`+ Stage as Playlist` + lazy `video_id`-correlated `LyricsPayload`), `views/settings.rs` (`config.toml` editor), `auth_store.rs` (`$XDG_RUNTIME_DIR` `0600` tmpfs cookie jar + orphan sweep), startup `LM` like hydration
  - **Phase 5 (IA & Polish — core wired):** `tab_bar.rs` (5 Primary Workspaces + Sync Staging Drawer toggle), `tab_router.rs`, `app.rs` (collapsible right-hand Sync Drawer + `Ctrl+K` Command Palette + `OsMediaBridge` polling), `command_palette.rs`, `theme.rs`, `icons.rs`, `interaction.rs` & `navigation.rs` (`Ctrl+1..5`, `Ctrl+K`, `Ctrl+Shift+S`, `Ctrl+Z`/`Ctrl+Shift+Z` redo, `Space`, `M`)
- **Evidence**:
  - `cargo check --workspace --all-targets`: exit 0
  - `cargo test --workspace`: exit 0 (`125/125` tests passing across `ytm-core`, `ytm-store`, `ytm-ops`, `ytm-enrich`, `ytm-cli`, `ytm-gui`)

---

## Previous Gate Status (2026-11-26 — Phase 0)

- `Phase 0 (Contract, Spec & Domain Model Lock)`: **PASS** `GLOSSARY.md` (12 canonical terms + forbidden synonyms), `ARCHITECTURE.md` (6-crate workspace + 3-lane worker + `mpv` IPC + `ArtworkCache`), `SPEC.md` (reconciled live weights, tables, `FetchManifest`, `MutationBatchReport`, `Configure -> Preview -> Verified POST`), and locked core/store/gui contracts (`P0-W1-T1..T2`, `P0-W2-T1..T3`)
- `Core / Store / Ops / Enrich / CLI / GUI Foundation`: **PASS** `ytm-core` (`P2-W1-T1..T3`, `P2-W2-T1`, `P4-W1-T1`), `ytm-store` (`P1-W1-T1`, `P2-W3-T1`, `P3-W1-T2`), `ytm-ops` (`P3-W1-T1`), `ytm-enrich` (`P3-W1-T3`), `ytm-cli` (`P2-W3-T3` + split E2E tests), `ytm-gui` (`P0-W2-T3`, `P1-W1-T2`)
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings, 0 new `#[allow(...)]`
- `cargo test --workspace`: **PASS** exit 0, `97/97` tests passed (`ytm-cli` 6, `ytm-core` 31, `ytm-enrich` 14, `ytm-gui` 20, `ytm-ops` 14, `ytm-store` 12)
- `wc -l`: **PASS** 0 files > 400 LOC across all 125 `.rs` files (`max = 400` in `crates/ytm-gui/src/views/inspect.rs`)

---

## 2026-11-26 — Phase 0 Contract Lock & Core/Store/Ops/Enrich/CLI/GUI-Foundation Re-Architecture

- **Touched areas (`path›symbol`)**:
  - `GLOSSARY.md`, `ARCHITECTURE.md`, `SPEC.md`: canonical 12-term glossary + forbidden synonyms, 6-crate workspace architecture, reconciled `dedupe.rs` weights, SQLite tables, and `Configure -> Preview -> Verified POST` / `FetchManifest` / `MutationBatchReport` contracts
  - `crates/ytm-core/src/{models,fetch_models,browse_models,http,auth,client,client_playlist,client_mutations,client_browse,traits,lib}.rs` & `parser/{mod,playlist,playlist_edit,track,browse,search}.rs`: `PlaylistAccess`, `FetchCompleteness`, `FetchManifest`, `MutationBatchReport`, `RetryPolicy`, `RwLock` API key recovery, `SAPISIDHASH` retry refresh, full pagination + `seen_tokens`, chunked `create_playlist`, locale-independent availability + dropped-renderer diagnostics, `SearchFilter`/`SearchPage`, `BrowsePage.continuation`, `LyricsPayload`
  - `crates/ytm-store/src/{models,traits,db,playlist_cache,snapshot,staged_ops,playback_session,lib}.rs`: idempotent schema migrations (`manifest_json`, `staged_ops`, `playback_session`), `PRAGMA busy_timeout = 5000`, `spawn_blocking` offloading, `CachedPlaylistRecord`, orphan pruning, transactional `staged_ops`, persistent `playback_session`
  - `crates/ytm-ops/src/{restore_plan,lib}.rs` & `crates/ytm-enrich/src/{models,normalize,splitter,lib}.rs`: shared `plan_restore_diff` / `compute_restore_moves`, NFKC/noise-stripped cache keys, `artist_cache_first` mode
  - `crates/ytm-cli/src/{cli,render,runner,mutations,restore}.rs` & `tests/`: `--force-incomplete` guard, `FetchManifest` & `MutationBatchReport` output, verified `added_items` ATV upgrades, shared `plan_restore_diff`, split E2E test modules
  - `crates/ytm-gui/src/{lib,main,state,state_forms,worker_msgs,sync_ops,sync_draft,sync_queue,sync_queue_tests}.rs`: `MutationPolicy`, `SessionState`, `DraftRevision`, `PlanFingerprint`, `JobId`, `DispatchEntryOutcome`, closed `StagedOp` set, dependency-safe `SyncQueue` + draft projections
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` (`97/97` PASS) exited 0 with 0 warnings, 0 new `#[allow(...)]`, and 0 `.rs` files > 400 LOC.

---

## Previous Gate Status (2026-11-16)

- `Gate 0A (Reference Audit)`: **PASS** `.refs/RESEARCH_NOTES.md` and 7 sanitized fixtures in `tests/fixtures/` verified; `.refs/` gitignored
- `Gate 0B (Workspace & Contract Lock)`: **PASS** exit 0, 0 warnings
- `Gate 1 (Core Infrastructure)`: **PASS** `ytm-core` & `ytm-store` 15 tests PASS, 0 warnings
- `Gate 2 (Ops & Enrich)`: **PASS** `ytm-ops` & `ytm-enrich` 15 tests PASS, 0 warnings (`31/31` workspace tests PASS)
- `Gate 3 (CLI & TUI)`: **PASS** `ytm-cli` wired across all 11 commands & 5-tab Ratatui TUI, `32/32` workspace tests PASS, 0 warnings
- `Gate 4 (E2E QA, Docs & Release)`: **PASS** E2E suite, `TestBackend` TUI render, `README.md`, and release build PASS (`34/34` workspace tests PASS, 0 warnings)
- `Gate 5 (Desktop GUI - ytm-gui)`: **PASS** `eframe`/`egui` cross-platform desktop studio + Chrome 2-line header parser, `35/35` workspace tests PASS, 0 warnings
- `Gate 6 (Sync Queue, Split-Diff, Drag-Drop & Bugfixes)`: **PASS** Central `Sync` queue + Kubuntu Copy/Move popup + Inspector column sort & LIS staging + Split Diff view + `LM`/localized count fix + MusicBrainz backoff/fallback, `38/38` workspace tests PASS, 0 warnings
- `Gate 7 (Runtime Auth, Browser Import & 20-Item UX/DnD Upgrade)`: **PASS** Runtime `INNERTUBE_API_KEY` discovery + `yt-dlp` Netscape `#HttpOnly_` browser import (`Brave`/`Chrome`/`Chromium`/`Firefox`) + 20-item DnD, auto-scroll, keyboard/accessibility, toast/undo, and resizable Split Diff upgrades, `44/44` workspace tests PASS, 0 warnings
- `Gate 8 (Spotify-Tier Audio Player, Artwork, Playlist Meta & Discover)`: **PASS** `mpv` IPC playback + `PlayQueue` + persistent player bar + LRU thumbnail cache (`image` 0.25) + playlist rename/description/privacy/delete + `♥`/`♡` Like + `Browse` (`FEmusic_home`/Artist/Album) + Radio & Lyrics (`next` + `MPLYt_*`), `49/49` workspace tests PASS, 0 warnings
- `Gate 9 (Non-Blocking mpv Actor, Premium Cookie Bridge, Secret Service & Quality UI)`: **PASS** background `mpv-actor` thread + `TableBuilder`/clip-rect UI fixes + `0600` Netscape cookie bridge (`web_music,tv,android`) + `secret-tool` OS keyring with `0600` fallback + silent `preferred_browser` refresh + `AudioQuality` selector, `50/50` workspace tests PASS, 0 warnings
- `Gate 10 (UI/UX Accessibility, Grouped Tabs, Persistent Order Sync & Repertoire File-Tree)`: **PASS** full-cell column `sort_header` (`A-Z`/`Z-A`) + 4-group color-coded responsive 2-row `tab_bar` with hover tooltips + prominent `"Save Order to YTM (N moves · tracks unchanged)"` LIS bar + `ytm-store` SQLite `playlist_cache` & paced `IndexAllPlaylists` + `[Lists | All Tracks]` sidebar search + horizontal split `repertoire_tree`/`repertoire_target` drag-drop (`Ask`/`Move`/`Copy`) + `Ctrl+Z` undo, `53/53` workspace tests PASS, 0 warnings
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace -- --test-threads=4`: **PASS** exit 0, 53/53 tests passed (`ytm-core`: 7 unit + 7 integration, `ytm-store`: 3 unit + 3 integration, `ytm-ops`: 3 unit + 5 integration, `ytm-enrich`: 1 unit + 7 integration, `ytm-cli`: 2 unit + 2 integration, `ytm-gui`: 13 unit)
- `cargo build --release --bin ytm-gui --bin ytm`: **PASS** exit 0 (`6.62s`, binaries `./target/release/ytm-gui` symlinked at `~/.local/bin/ytm-gui` & `./target/release/ytm`)
- `wc -l`: **PASS** all source `.rs` files <= 400 lines (`max = 400` in `crates/ytm-gui/src/views/inspect.rs`)

---

## 2026-11-16 — Wave 10 UI/UX Accessibility, Grouped Tabs, Persistent Order Sync & Repertoire File-Tree

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-store/src/{db,lib,playlist_cache}.rs`: `playlist_cache` SQLite table + `upsert_cached_playlist` / `load_all_cached_playlists` / `delete_cached_playlist` + roundtrip test
  - `crates/ytm-gui/src/{tab_bar,tab_router,state,interaction,navigation,overlays,sidebar,app,app_events,main}.rs`: 4-group color-coded responsive tab bar (`tab_bar.rs`) + hover tooltips + `[Lists | All Tracks]` search mode + `DragDropMode` (`Ask`/`Move`/`Copy`) + `Ctrl+Z` staged undo
  - `crates/ytm-gui/src/views/{inspect_sort,inspect_helpers,inspect,repertoire_tree,repertoire_target,split_pane,mod}.rs`: full-cell `sort_header` (`A-Z`/`Z-A`), prominent LIS `"Save Order to YTM"` action bar, horizontal File-Tree <-> Target Workspace drag-and-drop search
  - `crates/ytm-gui/src/{worker,worker_auth,worker_media,worker_msgs}.rs`: SQLite `PlaylistCacheHydrated` on `Bootstrap`, automatic cache upsert on `LoadPlaylist`, paced `350ms` `IndexAllPlaylists` worker task, `yt-dlp` `brave+gnomekeyring` & video URL cookie extraction fix
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -- --test-threads=4` (53/53 PASS), and `cargo build --release --bin ytm-gui --bin ytm` (6.62s) exited 0 with 0 warnings; max file length 400 lines (`<= 400`).

---

## 2026-11-15 — Wave 9 Non-Blocking mpv Actor, Premium Cookie Bridge & Auto Auth Refresh

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-gui/src/{player,player_ipc,views/queue_lyrics,views/player_bar,worker_media}.rs`: non-blocking `mpv-actor` thread (`PlayerController::configure`), `TableBuilder` + `set_clip_rect` + `^`/`v`/`x` glyph fixes in Queue & Lyrics / Player Bar, explicit unauth error handling
  - `crates/ytm-gui/src/auth_store.rs`: `AudioQuality` (`AutoPremium`, `BalancedOpus`, `DataSaver`), `save_credentials_secure` / `load_credentials_secure` (`secret-tool` + `0600` `auth.json` fallback), `write_mpv_cookie_jar` / `remove_mpv_cookie_jar_at` (`0600` `ytm-cookies.txt`), `save_gui_config`
  - `crates/ytm-gui/src/{state,worker,worker_auth,worker_msgs,app,app_events,tab_router,views/auth,main}.rs`: `preferred_browser` & `audio_quality` persistence in `config.toml`, silent `Bootstrap` browser refresh via `handle_bootstrap`, `AudioConfigUpdated` wiring to `PlayerController`, `Auth` tab quality selector + Cookie Bridge indicator
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -- --test-threads=4` (50/50 PASS), and `cargo build --release --bin ytm-gui --bin ytm` (4.59s) exited 0 with 0 warnings; max file length 386 lines (`<= 400`).

---

## 2026-11-15 — Wave 8 Spotify-Tier Audio Player, Artwork, Playlist Meta & Discover

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-core/src/{browse_models,models,traits,client,lib}.rs`, `crates/ytm-core/src/parser/{browse,track,mod}.rs`, `crates/ytm-core/tests/browse_and_mutations_test.rs`: `Track::thumbnail_url`, `MutationAction::{SetPlaylistName,SetPlaylistDescription,SetPlaylistPrivacy}`, `update_playlist_metadata`, `set_like_status`, `get_browse_page` (`FEmusic_home`/Artist/Album), `get_watch_next`, `get_lyrics`
  - `crates/ytm-gui/src/{player,player_ipc,play_queue,playback_ops,artwork,playlist_meta_modal,worker_media,worker_msgs,app_events}.rs`: `/usr/bin/mpv` Unix-socket IPC player + auto-advance, `PlayQueue` (`shuffle`, `repeat`, next/prev/move/remove), async LRU thumbnail cache (`image` 0.25 `jpeg`/`png` + `reqwest`), `PlaylistMetaModal` + confirmed `DeletePlaylist` + one-click `♥`/`♡` Like
  - `crates/ytm-gui/src/views/{player_bar,browse,queue_lyrics,inspect_menu,inspect_rows,inspect,inspect_actions,inspect_helpers}.rs` & `{app,sidebar,overlays,tab_router,worker,worker_exec,state,main}.rs`: bottom player bar, `Browse` tab + clickable Artist/Album links, `Queue & Lyrics` tab + `"Start Radio"`, row double-click/`Enter`/context-menu playback, row & player thumbnails
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -- --test-threads=4` (49/49 PASS), `cargo build --release --bin ytm-gui --bin ytm` (13.42s) exited 0 with 0 warnings.

---

## 2026-11-15 — Wave 7 Runtime Auth, Browser Import & 20-Item UX/DnD Upgrade

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-core/src/{http.rs,auth.rs,lib.rs}`: `ReqwestTransport::resolve_params` runtime `INNERTUBE_API_KEY` extraction from `music.youtube.com` (`OnceCell`), `netscape_cookies_to_header` (`#HttpOnly_` preservation + domain filter + `SAPISID` verification) + `netscape_cookies_to_header_preserves_httponly_and_filters_youtube`
  - `crates/ytm-gui/src/{main.rs,worker_auth.rs,worker.rs,worker_exec.rs,views/auth.rs}`: `.with_app_id("ytm-architect")`, Wayland repaint cadence, auto-switch to `Auth` tab on session failure, `BrowserKind` (`Brave` default, `Chrome`, `Chromium`, `Firefox`) + `yt-dlp --cookies-from-browser` one-click import, file import fallback
  - `crates/ytm-gui/src/{navigation.rs,interaction.rs,overlays.rs,app.rs,sidebar.rs,tab_router.rs,sync_queue.rs,sync_queue_tests.rs,state.rs}`: `NavHistory` (`Alt+◀/▶` & mouse `Extra1/Extra2`), `ToastState` (6s TTL, `Undo`, `Sync →`, `✕`), global shortcuts (`Ctrl+1..9`, `Ctrl+F`, `Ctrl+Enter`, `Ctrl+/-/0` zoom), `handle_cross_playlist_drop` (`Ctrl`=Copy, `Shift`=Move), modal backdrop + `Esc`/`Enter` + error recovery (`Go to Auth →`, `Retry`), `SyncQueue::move_entry` & `last_entry_id`, sidebar keyboard nav & edge auto-scroll
  - `crates/ytm-gui/src/views/{inspect,inspect_rows,inspect_actions,inspect_helpers,split_diff,split_pane,set_ops,sort,sync,dedupe,health,genres_snapshots,mod}.rs`: full-row DnD + ghost badge + bottom drop slot + disabled tooltip + `Grab`/`Grabbing` cursors, `Shift+Click` range select + `Ctrl+A`/`Delete`/`Esc`/`Home`/`End`/`Up`/`Down`/`Space` + `# Go`, full-row context menu (`Open in Browser`, `Filter by Artist`, batch `Copy/Move to Playlist`), Quick Search & Set Ops DnD, symmetric Split Diff search + draggable vertical splitter (`0.25..=0.75`) + `⇄ Swap A/B` + multi-select drag, `Sort`/`Sync` `↑`/`↓` reordering, double-click snapshot inspect
  - `scripts/install-desktop.sh`: symlinks `~/.local/bin/{ytm-gui,ytm}` and installs `~/.local/share/applications/ytm-architect.desktop`
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (44/44 PASS), and `cargo build --release --bin ytm-gui --bin ytm` (3.62s) exited 0 with 0 warnings.

---

## 2026-11-15 — Wave 6 Sync Queue, Split-Diff, Drag-Drop & Bugfixes

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-core/src/parser/playlist.rs`: `looks_like_track_count` localized/formatted sidebar count parsing + `parse_library_playlists_handles_localized_and_formatted_counts` unit test
  - `crates/ytm-enrich/src/{genre_split,splitter,lib}.rs` & `crates/ytm-enrich/tests/enrich_test.rs`: MusicBrainz `429`/`503` bounded backoff retry + graceful cache/Deezer/Last.fm degradation + `musicbrainz_rate_limit_retries_and_gracefully_falls_back`
  - `crates/ytm-gui/src/{sync_queue,sync_queue_tests,sidebar,overlays,tab_router,state,app,worker,worker_exec,worker_plans}.rs`: central `SyncQueue` staging & working-draft projection, playlist cache with count backfill, sidebar DND + Kubuntu-style Copy/Move popup, bottom batch progress bar
  - `crates/ytm-gui/src/views/{inspect,inspect_helpers,split_diff,sync,dedupe,sort,set_ops,health,genres_snapshots,mod}.rs`: clickable A-Z/Z-A Inspector columns + 1-click LIS stage, row drag reorder, Quick YTM search & add, Split Diff A↔B view, dedicated `Sync` tab routing all tabs through pre-sync snapshots
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (38/38 PASS), and `cargo build --release --bin ytm-gui --bin ytm` (7.43s) exited 0 with 0 warnings.

---

## 2026-11-15 — Wave 5 Pure-Rust Desktop GUI (`ytm-gui`)

- **Touched areas (`path›symbol`)**:
  - `Cargo.toml`, `Cargo.lock`: added `crates/ytm-gui`, pinned `eframe = "0.31"` and `egui_extras = "0.31"`
  - `crates/ytm-core/src/auth.rs`: `is_header_key_line`, `parse_header_text_auth` support for Chrome DevTools alternating 2-line `key\nvalue` paste format + unit test
  - `crates/ytm-gui/src/{main.rs,app.rs,state.rs,worker.rs,worker_exec.rs}`: 60 FPS `eframe` shell, `AsyncState<T>`, `PendingMutation` modal confirmation gate, Tokio worker thread with mandatory `save_snapshot` before every remote mutation
  - `crates/ytm-gui/src/views/{mod,inspect,dedupe,sort,set_ops,health,genres_snapshots,auth}.rs`: 8 visual tabs covering Inspector/Backup, Dedupe, LIS Minimal-Move Sort, Set Algebra, Health & Official ATV Upgrades, Genre Split, Snapshots Time-Machine, and Auth Import
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (35/35 PASS), and `cargo build --release --bin ytm-gui --bin ytm` (58.21s) exited 0 with 0 warnings.
- **Remaining gaps**: Manual desktop launch of `./target/release/ytm-gui` on user's Wayland/X11 display.

---

## 2026-11-15 — Wave 4 E2E Integration QA, Release Build & Documentation (`ytm-architect`)

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-cli/Cargo.toml`: pinned `tempfile.workspace = true` under `[dev-dependencies]`
  - `crates/ytm-cli/tests/e2e_pipeline_test.rs`: `StatefulMockClient`, `MockVerifier`, `e2e_cli_dry_run_mutate_snapshot_restore_and_idempotent_genre_split`, `e2e_health_upgrade_and_headless_tui_tabs` (all 5 `ActiveTab` frames verified with `TestBackend`)
  - `README.md`: cross-platform Linux & Windows (PowerShell) build, `ProjectDirs` paths, DevTools/`browser.json` auth import, Brand Account `X-Goog-PageId` guide, safety defaults, CLI command reference
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` (34/34 PASS), and `cargo build --release` (57.65s) all exited 0 with 0 warnings; `./target/release/ytm --help` verified.
- **Remaining gaps**: None (all planned waves 0A–4 complete).

---

## 2026-11-15 — Wave 3 CLI Runner & Ratatui TUI (`ytm-cli`)

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-cli/src/cli.rs`: `From<CliExportFormat>`, `From<CliKeepStrategy>`, `From<CliSetOp>` conversions + dry-run parser defaults
  - `crates/ytm-cli/src/config.rs`: `AppConfig`, `load_app_config`, `load_genre_matcher`, `YTM_LASTFM_API_KEY` env override
  - `crates/ytm-cli/src/render.rs`: `comfy_table` UTF-8 renderers (`list`, `inspect`, `snapshots`, `dedupe`, `sort`, `set`, `health`, `split-genre`) + `indicatif` progress bar
  - `crates/ytm-cli/src/mutations.rs`: `handle_dedupe`, `handle_sort`, `handle_set`, `handle_health` (dry-run default + mandatory pre-mutation snapshots)
  - `crates/ytm-cli/src/restore.rs`: `handle_split_genre`, `handle_snapshot_restore` (with LIS reorder + pre-restore snapshot)
  - `crates/ytm-cli/src/runner.rs`: `RuntimeContext`, `SessionVerifier`, `LiveSessionVerifier`, `import_and_verify_auth`, `execute_command` across all 11 subcommands
  - `crates/ytm-cli/src/tui.rs` & `crates/ytm-cli/src/tui_render.rs`: 5-tab `TuiApp` (`Playlists`, `Dedupe`, `Sort`, `Health`, `Genres`), Windows/Linux `KeyEventKind::Press` handling, Crossterm guard, and `TestBackend` support
  - `crates/ytm-cli/src/main.rs`: `AppPaths`, `SqliteStore`, `InnerTubeClient`, `EnrichmentService`, and stdin/file `auth import` wiring
- **Evidence**:
  - `cargo fmt --all -- --check`, `wc -l crates/ytm-cli/src/*.rs` (max 317L), `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` (32/32 PASS) exited 0 with 0 warnings.
- **Remaining gaps**: Ready to execute Wave 4 (`crates/ytm-cli/tests/e2e_pipeline_test.rs`, cross-platform `README.md`, `cargo build --release`).

---

## 2026-11-15 — Wave 2 Algorithms & Metadata Enrichment (`ytm-ops` & `ytm-enrich`)

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-ops/src/normalize.rs`: `normalize_track` (`NFKC`, `UGC` `"Artist - Title"` splitting, `&` -> `and`, noise-tag stripping)
  - `crates/ytm-ops/src/dedupe.rs`: `keeper_score`, `plan_dedupe` (Pass 1 exact `video_id`, Pass 2 Jaro-Winkler + duration guard, `KeeperStrategy`, `skipped_without_set_id`)
  - `crates/ytm-ops/src/sort.rs`: `compute_minimal_moves` (`O(N log N)` Patience LIS + reverse suffix emit), `apply_move_actions`, `plan_sort` (multi-key comparator + deterministic `artist_spread` shuffle)
  - `crates/ytm-ops/src/set_ops.rs`: `plan_set_op` (`Diff`, `Intersect`, `Merge` under `ExactVideoId` & `FuzzyNormalized`)
  - `crates/ytm-ops/src/health.rs`: `scan_playlist_health` (unavailable flags, missing `setVideoId`, `UGC/OMV -> ATV` upgrade candidates)
  - `crates/ytm-ops/tests/ops_algorithms_test.rs`: 5 integration tests (`5! = 120` permutations, 100-item LIS, deterministic artist-spread shuffle, UGC/ATV dedupe, set algebra + health)
  - `crates/ytm-enrich/src/rate_limit.rs`: `PacedLimiter` (`tokio::time` interval enforcement)
  - `crates/ytm-enrich/src/taxonomy.rs`: `GenreMatcher` (`genres.toml` longest-pattern-first compiler & `classify`)
  - `crates/ytm-enrich/src/providers.rs`: `EnrichmentHttp`, `ReqwestEnrichmentHttp`, `MockEnrichmentHttp`, MusicBrainz/Last.fm/Deezer/ListenBrainz parsers
  - `crates/ytm-enrich/src/splitter.rs`: `EnrichmentService` (`MetadataProvider` with SQLite TTL cache before HTTP) and idempotent `plan_genre_split`
  - `crates/ytm-enrich/tests/enrich_test.rs`: 6 integration tests (0-HTTP cache hit, cache miss -> MusicBrainz -> SQLite reuse, Deezer fallback, `PacedLimiter`, `GenreMatcher`, idempotent genre split)
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ytm-ops -p ytm-enrich`, and `cargo test --workspace` (31/31 PASS) exited 0 with 0 warnings.
- **Remaining gaps**: Ready to execute Wave 3 (`ytm-cli` & Ratatui TUI).

---

## 2026-11-15 — Wave 1 Core Infrastructure (`ytm-core` & `ytm-store`)

- **Touched areas (`path›symbol`)**:
  - `crates/ytm-core/src/auth.rs`: `parse_auth_input`, `extract_sapisid`, `compute_sapisidhash`, `build_auth_headers`
  - `crates/ytm-core/src/parser/{mod,tree,playlist,track}.rs`: recursive JSON tree helpers, canary logged-out detection, `FEmusic_liked_playlists`, 2025 + legacy continuation parsing, `playlistItemData.playlistSetVideoId` extraction, search & edit response parsing
  - `crates/ytm-core/src/http.rs`: `ReqwestTransport` (`rustls-tls`, 429/503 backoff) + `MockTransport`
  - `crates/ytm-core/src/client.rs`: `InnerTubeClient` (`InnerTubeClientTrait`, sibling `context.user`, 50-item chunking)
  - `crates/ytm-core/tests/fixtures_and_client_test.rs`: 5 deterministic InnerTube integration tests
  - `crates/ytm-store/src/paths.rs`: `AppPaths` (`ProjectDirs` + `from_root`), atomic writes, Unix `0600` secret file permissions
  - `crates/ytm-store/src/db.rs`: `SqliteStore` migrations + `EnrichmentCacheStore` (`track_cache`, `artist_genres`, `track_enrichment`, `snapshots`)
  - `crates/ytm-store/src/snapshot.rs`: `SnapshotStore` (disk atomic JSON + SQLite index)
  - `crates/ytm-store/src/export.rs`: `.json`, `.csv`, `.m3u8` export & JSON import
  - `crates/ytm-store/tests/store_integration_test.rs`: `:memory:` SQLite, `TempDir` permissions, snapshot & export roundtrips
- **Evidence**:
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` (20/20 PASS) exited 0 with 0 warnings.
- **Remaining gaps**: Ready to plan & execute Wave 2 (`Worker-Ops` || `Worker-Enrich`).

---

## 2026-11-15 — Wave 0A Reference Audit & Wave 0B Workspace Contract Lock

- **Touched areas (`path›symbol`)**:
  - `.gitignore`, `LICENSE-MIT`, `LICENSE-APACHE`, `AGENT.md`, `CLAUDE.md`, `ARCHITECTURE.md`, `SPEC.md`, `MEMORY.md`, `PROGRESS.md`, `README.md`
  - `.refs/RESEARCH_NOTES.md` (gitignored), `tests/fixtures/*.json` (7 sanitized InnerTube response fixtures)
  - `config/config.example.toml`, `config/genres.example.toml`
  - `Cargo.toml`, `crates/ytm-core/src/*`, `crates/ytm-store/src/*`, `crates/ytm-ops/src/*`, `crates/ytm-enrich/src/*`, `crates/ytm-cli/src/*`
- **Evidence**:
  - Rust toolchain installed (`rustc 1.100.0`, `cargo 1.100.0`, `clippy 0.1.100`, `rustfmt 1.11.0-stable`).
  - `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` passed with exit 0 and 0 warnings (9/9 unit tests passing).
- **Remaining gaps**: Superseded by Wave 1 completion above.
