# PROGRESS.md

Sole authority for dates, verification gate statuses, and operational session history.

---

## Current Gate Status (2026-11-15)

- `Gate 0A (Reference Audit)`: **PASS** `.refs/RESEARCH_NOTES.md` and 7 sanitized fixtures in `tests/fixtures/` verified; `.refs/` gitignored
- `Gate 0B (Workspace & Contract Lock)`: **PASS** exit 0, 0 warnings
- `Gate 1 (Core Infrastructure)`: **PASS** `ytm-core` & `ytm-store` 15 tests PASS, 0 warnings
- `Gate 2 (Ops & Enrich)`: **PASS** `ytm-ops` & `ytm-enrich` 15 tests PASS, 0 warnings (`31/31` workspace tests PASS)
- `Gate 3 (CLI & TUI)`: **PASS** `ytm-cli` wired across all 11 commands & 5-tab Ratatui TUI, `32/32` workspace tests PASS, 0 warnings
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace`: **PASS** exit 0, 32/32 tests passed (`ytm-core`: 5 unit + 5 integration, `ytm-store`: 2 unit + 3 integration, `ytm-ops`: 3 unit + 5 integration, `ytm-enrich`: 1 unit + 6 integration, `ytm-cli`: 2 unit)
- `wc -l crates/*/src/**/*.rs`: **PASS** max 390 lines (`crates/ytm-enrich/src/splitter.rs`), all <= 400

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
