# ARCHITECTURE.md

## Workspace Topology

```text
ytm-architect/
├── Cargo.toml                  # 6-crate workspace manifest & shared dependencies
├── GLOSSARY.md                 # Canonical domain terms & forbidden synonyms
├── config/
│   ├── config.example.toml     # Client context, pacing, enrichment provider settings
│   └── genres.example.toml     # Hierarchical subgenre -> parent genre rules
├── crates/
│   ├── ytm-core/               # Auth signing, InnerTube HTTP + MockTransport, JSON tree parser, browse_models, pagination, FetchManifest, chunked mutations, MutationBatchReport
│   ├── ytm-store/              # ProjectDirs paths, SQLite cache + migrations (track/genre/enrichment/playlist_cache/snapshots/staged_ops/playback_session), JSON/CSV/M3U8 export/import
│   ├── ytm-ops/                # Normalization, exact/fuzzy deduper, multi-key sorter + LIS minimal-move planner, set algebra, health scanner, shared restore diff planner
│   ├── ytm-enrich/             # Rate-limited MusicBrainz + optional Last.fm/Deezer/ListenBrainz + genres.toml mapper + split-genre planner
│   ├── ytm-cli/                # Clap CLI, comfy-table dry-run diff renderer, indicatif progress, Ratatui TUI
│   └── ytm-gui/                # eframe/egui desktop workspace, 3-lane async Tokio worker, mpv IPC player, ArtworkCache, MPRIS2/SMTC controls, SyncQueue & DraftRevision cache
└── tests/
    └── fixtures/               # Sanitized InnerTube JSON response payloads for deterministic tests
```

## Crate Dependency Graph & Layer Flow

```text
ytm-gui (eframe/egui + 3-lane worker + mpv IPC + ArtworkCache + MPRIS2/SMTC) ──┐
ytm-cli (Clap CLI + Ratatui TUI)                                              ──┼──> ytm-ops (dedupe, LIS sort, set ops, health, restore diff)
                                                                                ├──> ytm-enrich (metadata cache + external providers -> ytm-store, ytm-core)
                                                                                ├──> ytm-store (SQLite persistence, snapshots, staged_ops, playback_session -> ytm-core)
                                                                                └──> ytm-core (InnerTube HTTP/2, SAPISIDHASH, tree parser, FetchManifest)
```

- **`ytm-core`** depends on zero sibling workspace crates. Exposes `InnerTubeClientTrait` and `HttpTransport` (`ReqwestTransport` / `MockTransport`), recursive JSON tree parsers (`parser/browse.rs`, `parser/playlist.rs`, `parser/track.rs`, `parser/tree.rs`), `PlaylistAccess`, `FetchManifest`, and `MutationBatchReport`.
- **`ytm-store`** depends only on `ytm-core` domain models (`Playlist`, `Track`, `MusicVideoType`, `FetchManifest`). Owns OS path resolution (`directories::ProjectDirs`), SQLite schema migrations (`track_cache`, `artist_genres`, `track_enrichment`, `snapshots`, `playlist_cache`, `staged_ops`, `playback_session`), and atomic pre-mutation snapshots.
- **`ytm-ops`** depends only on `ytm-core` domain models. Contains pure, deterministic planning functions (`plan_dedupe`, `plan_sort`, `plan_set_op`, `scan_playlist_health`, `plan_restore_diff`) with zero network or filesystem I/O.
- **`ytm-enrich`** depends on `ytm-core` and `ytm-store` (`EnrichmentCacheStore`). Checks local SQLite cache before querying rate-limited external providers and supports `artist_cache_first` short-circuiting.
- **`ytm-cli`** wires CLI subcommands, enforces `--dry-run` defaults and `FetchCompleteness` guards, executes pre-mutation snapshots through `ytm-store` before mutating calls on `InnerTubeClientTrait`, and hosts the `ratatui` interactive dashboard.
- **`ytm-gui`** renders the `eframe`/`egui` desktop application backed by:
  - **3-Lane Async Worker (`WorkerLane`):** `tokio::sync::mpsc` dispatcher splitting work across `Interactive` (parallel browse/search/lyrics/playlist loads), `BackgroundJob` (cancellable indexing, backup, health, enrichment via `JobId`), and `MutationSerial` (serialized `SyncQueue` POST dispatch with pre-flight snapshot and read-back verification).
  - **`mpv` IPC Player (`player.rs` / `player_ipc.rs`):** Manages a local `mpv` child process over JSON IPC (`UnixStream` on Unix, Named Pipe on Windows) using `yt-dlp` stream resolution with `0600` runtime cookie jars and MPRIS2/SMTC OS media-control integration.
  - **`ArtworkCache` (`artwork.rs`):** Bounded concurrent HTTP thumbnail fetcher (`Semaphore`) backed by an LRU GPU texture cache and bounded SHA-256 disk cache under `ProjectDirs::cache_dir()/artwork/` with failure retry TTL.
  - **Memoized Draft Layer (`SyncQueue` + `DraftCache`):** Projects staged edits onto cached playlists keyed by `(playlist_id, DraftRevision)` so steady-state frames avoid full-playlist clones and LIS recomputation.

## External Services

| Service / Runtime | Purpose | Pacing / Constraints |
| :--- | :--- | :--- |
| YouTube Music InnerTube (`music.youtube.com/youtubei/v1/`) | Session canary, library browse, search, watch-next/lyrics, playlist mutations, like status | `SAPISIDHASH`, HTTP/2 64 KiB header cap, max 50 actions/POST, 600ms delay, jittered backoff on `429`/`503`, `409`/`400` post-create retry |
| MusicBrainz (`/ws/2/recording`, `/ws/2/artist`) | Primary canonical genre/tag & release-year enrichment | `>= 1.1s` interval per request, compliant `User-Agent` |
| Last.fm / Deezer / ListenBrainz (optional) | Fallback genre & release metadata when MusicBrainz misses | Enabled via `config.toml` or env vars; cached in SQLite |
| Local `mpv` + `yt-dlp` (GUI playback) | Audio stream resolution & gapless IPC playback | JSON IPC (`UnixStream` / Windows Named Pipe), `0600` runtime cookie jar, silent startup browser cookie extraction disabled by default |
