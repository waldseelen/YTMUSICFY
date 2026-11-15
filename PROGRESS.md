# PROGRESS.md

Sole authority for dates, verification gate statuses, and operational session history.

---

## Current Gate Status (2026-11-15)

- `Gate 0A (Reference Audit)`: **PASS** `.refs/RESEARCH_NOTES.md` and 7 sanitized fixtures in `tests/fixtures/` verified; `.refs/` gitignored
- `Gate 0B (Workspace & Contract Lock)`: **PASS** exit 0, 0 warnings
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace`: **PASS** exit 0, 20/20 tests passed (`ytm-core`: 5 unit + 5 integration, `ytm-store`: 2 unit + 3 integration, `ytm-ops`: 3, `ytm-enrich`: 1, `ytm-cli`: 1)
- `wc -l crates/*/src/**/*.rs`: **PASS** max 309 lines (`crates/ytm-core/src/parser/playlist.rs`), all <= 400

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
