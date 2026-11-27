# ARCHITECTURE.md

Architectural blueprint and code locator for **Dalga** (internal workspace `ytm-architect`: binaries `ytm-gui` and `ytm`). Keep crate boundaries strict and source files `<= 400` lines.

---

## 1. Workspace Topology

```text
YTMUSICFY/
├── Cargo.toml                   # 6-crate workspace manifest & shared dependencies
├── CLAUDE.md                    # Binding rules & read order
├── SPEC.md / GLOSSARY.md        # Domain contracts & canonical terminology
├── TASKS.md                     # Active v0.2.0–v0.5.0 roadmap & locked product decisions
├── PROGRESS.md                  # Dates, releases, and verified gate outputs
├── assets/brand/                # Dalga Concept A monochrome 3D PNGs, .ico, .desktop, drafts/
├── config/                      # config.example.toml, genres.example.toml
├── tests/fixtures/              # Sanitized InnerTube JSON payloads for deterministic tests
└── crates/
    ├── ytm-core/                # InnerTube HTTP/2 client, SAPISIDHASH auth, recursive parsers, FetchManifest
    ├── ytm-store/               # ProjectDirs paths, SQLite persistence, snapshots, export/import
    ├── ytm-ops/                 # Pure deterministic planning (dedupe, LIS sort, set ops, health, restore diff)
    ├── ytm-enrich/              # Rate-limited MusicBrainz / Last.fm / Deezer / ListenBrainz genre & year splitter
    ├── ytm-cli/                 # Clap CLI + Ratatui TUI
    └── ytm-gui/                 # eframe/egui desktop app, 3-lane Tokio worker, mpv IPC player, MPRIS2/SMTC
```

---

## 2. Crate Dependency Graph & Layer Flow

```text
ytm-gui (eframe/egui + 3-lane worker + mpv IPC + MPRIS2/SMTC) ──┐
ytm-cli (Clap CLI + Ratatui TUI)                               ──┼──> ytm-ops (pure planners)
                                                                 ├──> ytm-enrich -> ytm-store, ytm-core
                                                                 ├──> ytm-store  -> ytm-core
                                                                 └──> ytm-core   (zero sibling deps)
```

- **`ytm-core`**: zero sibling crate dependencies; owns `InnerTubeClientTrait`, `HttpTransport`, recursive JSON tree search, `PlaylistAccess`, `FetchManifest`, chunked playlist mutations (`<= 50` actions/POST).
- **`ytm-store`**: depends only on `ytm-core` models; owns `ProjectDirs::from("io", "ytm-architect", "ytm-architect")`, SQLite migrations, `0600` secret file writes, atomic pre-mutation snapshots, `staged_ops`, `playlist_cache`, `playback_session`.
- **`ytm-ops`**: depends only on `ytm-core` models; pure deterministic algorithms with zero network/FS IO.
- **`ytm-enrich`**: checks `ytm-store` SQLite cache before rate-limited external metadata queries.
- **`ytm-cli`**: CLI subcommands (`--dry-run` default), pre-mutation snapshot guard, Ratatui TUI.
- **`ytm-gui`**: desktop UI, `Brand`, `Theme`, `mpv` IPC playback, `ArtworkCache`, and 3-lane `WorkerLane` backend.

---

## 3. Code Locator (Where to Find Everything)

### `crates/ytm-core/src/`
- `models.rs`, `browse_models.rs`, `fetch_models.rs`, `error.rs`, `traits.rs`: core domain types (`Track`, `Playlist`, `PlaylistAccess`, `FetchManifest`, `SearchPage`, `BrowsePage`, `LyricsPayload`) and `InnerTubeClientTrait`.
- `auth.rs`, `http.rs`: `SAPISIDHASH` signing, Netscape cookie conversion, `ReqwestTransport` (`rustls-tls`) / `MockTransport`.
- `client.rs`, `client_playlist.rs`, `client_browse.rs`, `client_mutations.rs`: InnerTube endpoints and chunked POST execution.
- `parser/{tree,track,playlist,playlist_edit,browse,browse_cards,search}.rs`: anti-fragile recursive JSON extractors (never hardcode array indices).

### `crates/ytm-store/src/`
- `paths.rs`, `models.rs`, `traits.rs`, `error.rs`: `AppPaths`, `write_atomic`, `write_secret_file` (`0600` on Unix).
- `db.rs`, `offline_cache.rs`, `playlist_cache.rs`, `snapshot.rs`, `staged_ops.rs`, `playback_session.rs`, `export.rs`: SQLite schema (`v4`), snapshot store, JSON/CSV/M3U8 export/import.

### `crates/ytm-ops/src/`
- `models.rs`, `normalize.rs`, `dedupe.rs`, `sort.rs`, `set_ops.rs`, `health.rs`, `restore_plan.rs`: pure planners (`plan_dedupe`, `plan_sort` LIS minimal moves, `plan_set_op`, `scan_playlist_health`, `plan_restore_diff`).

### `crates/ytm-enrich/src/`
- `models.rs`, `error.rs`, `traits.rs`, `normalize.rs`, `rate_limit.rs`, `providers.rs`, `taxonomy.rs`, `genre_split.rs`, `splitter.rs`.

### `crates/ytm-cli/src/`
- `main.rs`, `lib.rs`, `cli.rs`, `config.rs`, `runner.rs`, `mutations.rs`, `restore.rs`, `render.rs`, `tui.rs`, `tui_render.rs`.

### `crates/ytm-gui/src/`
- **App Shell & Navigation:** `main.rs`, `lib.rs` (Wayland `vsync` guard + window icon), `app.rs` (`YtmGuiApp`), `app_frame.rs` (`sync_theme_and_input`, `finish_frame`), `app_events.rs` (`apply_worker_event`), `tab_bar.rs`, `tab_router.rs`, `overlays.rs`, `navigation.rs`, `interaction.rs`, `shortcuts.rs`, `command_palette.rs`, `sidebar.rs`.
- **Tray, Window Lifecycle & Mini Player:** `tray.rs` (`TrayBackend`, `TrayController`), `tray_linux.rs` (`ksni`), `tray_windows.rs` (`tray-icon`), `window_lifecycle.rs` (`resolve_close_request`), `mini_player.rs` (edge snap, monitor clamping, idle opacity).
- **Brand, Theme & Icons:** `brand.rs` (centralized `Dalga` strings & embedded Concept A logos), `contracts.rs` (`ThemeMode`, `AccentPalette`, `UiLang`, `MiniPlayerMode`, `CloseAction`, `StatusPriority`, `StatusMessage`), `theme.rs`, `icons.rs`.
- **Credentials & Config:** `auth_store.rs` (`0600` `auth.json` primary, `YTM_USE_SECRET_SERVICE=1` opt-in, `$XDG_RUNTIME_DIR` tmpfs `mpv` cookie jar).
- **Playback & OS Media:** `player.rs`, `player_actor.rs`, `player_process.rs`, `player_ipc.rs`, `play_queue.rs`, `playback_ops.rs`, `media_controls.rs` (`souvlaki` MPRIS2 / Win32 `HWND` SMTC), `artwork.rs` (bounded HTTP + 50 MiB disk LRU).
- **State & Sync:** `state.rs`, `state_forms.rs`, `sync_queue.rs`, `sync_ops.rs`, `sync_draft.rs`, `draft_cache.rs`, `playlist_meta_modal.rs`.
- **3-Lane Async Worker (`worker*.rs`):**
  - `worker.rs`: `Interactive` / `BackgroundJob` / `MutationSerial` dispatcher.
  - `worker_msgs.rs`: `WorkerCmd` & `WorkerEvent` contracts.
  - `worker_auth.rs`: `yt-dlp` `0700` temp-dir browser import + off-thread Wayland clipboard import (`wl-paste`).
  - `worker_media.rs`, `worker_jobs.rs`, `worker_plans.rs`, `worker_exec.rs`, `worker_sync.rs`, `worker_sync_ops.rs`.
- **Views (`views/`):**
  - Core & Home: `auth.rs`, `browse.rs`, `home_cards.rs`, `search.rs`, `search_dropdown.rs`, `queue_lyrics.rs`, `player_bar.rs`, `mini_player_view.rs`, `mini_player_search.rs`, `settings.rs`, `components.rs`, `context_menu.rs`, `context_menu_model.rs`.
  - Library & Repertoire: `inspect.rs`, `inspect_toolbar.rs`, `inspect_helpers.rs`, `inspect_rows.rs`, `inspect_actions.rs`, `inspect_sort.rs`, `repertoire_tree.rs`, `repertoire_target.rs`, `repertoire_batch.rs`, `split_diff.rs`, `split_pane.rs`.
  - Studio & Safety: `dedupe.rs`, `sort.rs`, `set_ops.rs`, `health.rs`, `genres_snapshots.rs`, `sync.rs`.

---

## 4. Runtime & Cross-Platform Invariants

- **3-Lane Worker:** UI never blocks on HTTP, `yt-dlp`, `wl-paste`, or SQLite writes; `MutationSerial` enforces `Snapshot -> Pre-flight Drift Check -> Chunked POST -> Read-Back FetchManifest`.
- **Wayland Safety:** `lib.rs` disables blocking GL `vsync` on Linux Wayland (`WAYLAND_DISPLAY`) so `eglSwapBuffers` never stalls Hyprland `xdg_wm_base.ping` on hidden workspaces; `app_events.rs` & `player_ipc.rs` throttle `time-pos` repaints (`200ms` active -> `1500ms` unfocused/minimized).
- **Credential Safety:** `auth_store.rs` reads/writes `0600` `auth.json` directly and never spawns `secret-tool` unless `YTM_USE_SECRET_SERVICE=1` is set (avoids GNOME Keyring modal prompts on autologin desktops).

---

## 5. External Services

| Service | Purpose | Constraints / Pacing |
| :--- | :--- | :--- |
| YouTube Music InnerTube (`music.youtube.com/youtubei/v1/`) | Canary, browse, search, watch-next/lyrics, mutations | `SAPISIDHASH`, `<= 50` actions/POST, 600ms pacing, jittered backoff |
| MusicBrainz / Last.fm / Deezer / ListenBrainz | Genre & release-year enrichment | `>= 1.1s` MusicBrainz interval; SQLite cached |
| Local `mpv` + `yt-dlp` | Audio playback & browser cookie import | JSON IPC (`UnixStream` / Named Pipe), `0600` runtime cookie jar |
