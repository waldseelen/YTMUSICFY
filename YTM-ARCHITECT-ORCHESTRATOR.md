




# MASTER ORCHESTRATOR BLUEPRINT: YTM-ARCHITECT (RUST)

> **Purpose:** This document is the single source of truth for an **Orchestrator Agent** supervising multiple specialized **Worker Agents** to design, verify, and build **`ytm-architect`**—a local-first, cross-platform (**Windows & Linux**), high-performance YouTube Music library & playlist engineering suite written in pure Rust.

---

## 1. ORCHESTRATOR PROTOCOL & MULTI-AGENT EXECUTION MODEL

You are the **Principal Systems Orchestrator**. You coordinate small, focused **Worker Agents** rather than writing monolithic code in a single context.

### 1.1. Non-Negotiable Engineering Rules
1. **Contract-First, Wave-Based Parallelism:**
   - First inspect open-source reference repositories (Wave 0A) and lock all shared domain models, error types, and trait boundaries across crates (Wave 0B).
   - Once Wave 0 passes compilation, fan out independent crate implementations to parallel Workers with **strict, non-overlapping file ownership**.
   - Never allow two Workers to modify the same file concurrently.
2. **Verification Gates Between Waves:**
   - No wave is complete until the Orchestrator (or QA Worker) runs and passes:
     - `cargo fmt --all -- --check`
     - `cargo check --workspace --all-targets`
     - `cargo clippy --workspace --all-targets -- -D warnings`
     - `cargo test --workspace`
3. **Zero-Placeholder Policy:**
   - Never commit `todo!()`, `unimplemented!()`, `// ...`, or silent stubs in production paths. Every committed function must be complete, error-handled, and tested.
4. **Safety-First Mutations (`--dry-run` + Pre-Flight Snapshots):**
   - Every command or operation that mutates remote YouTube Music state (`dedupe`, `sort`, `split-genre`, `set --into`, `health-fix`, `restore`, `delete`) **MUST default to dry-run preview mode** unless `--execute` is explicitly provided.
   - Before sending **any** mutating HTTP POST (`edit_playlist`, `create_playlist`, `delete_playlist`), the engine **MUST** write an atomic local snapshot (JSON + SQLite record) of the target playlist(s) to the platform data directory.
5. **Anti-Fragile InnerTube JSON Parsing:**
   - Google `youtubei/v1` responses are deeply nested, polymorphic JSON trees that shift over time.
   - **Never** hardcode brittle index chains such as `json["contents"]["twoColumnBrowseResultsRenderer"][0]...`.
   - Implement recursive tree-search helpers (`find_all_by_key`, `find_first_by_key`, `extract_runs_text`) over `serde_json::Value`, backed by unit tests using sanitized real-world JSON fixtures in `tests/fixtures/`.
6. **Cross-Platform First-Class Support (Windows & Linux):**
   - Use `directories::ProjectDirs` (`"io"`, `"ytm-architect"`, `"ytm-architect"`) for all config, cache, and snapshot paths. **Never** hardcode `~/.config`, `~/.local/share`, or OS-specific path separators.
   - Use `reqwest` with `rustls-tls` (`default-features = false`) so builds on both Windows (MSVC/GNU) and Linux require zero system OpenSSL dependencies.
   - Handle file permissions cleanly per OS (`#[cfg(unix)]` `0600` on `auth.json`; `#[cfg(windows)]` user-scoped warning/ACL check).

---

## 2. PRODUCT VISION & FEATURE PRIORITIES

YouTube Music lacks bulk library curation, smart duplicate detection across media versions (`Official Audio` vs. `Official Music Video` vs. `User Upload`), multi-key in-place sorting, set algebra (`diff`, `intersect`, `merge`), dead-track cleanup, and reliable genre partitioning. Meanwhile, the official YouTube Data API v3 enforces a tight 10,000-unit daily quota and blocks access to YouTube Music's `Liked Music` (`LM`) collection.

**`ytm-architect`** brings Spotify-ecosystem-grade playlist engineering to YouTube Music via direct `youtubei/v1` (InnerTube) integration, local SQLite caching, multi-source metadata enrichment, and a low-latency Rust CLI + Ratatui TUI.

### Priority Tiers
- **Tier 1 (Core — Must Ship Complete in Waves 0–4):**
  1. Resilient InnerTube Auth (`SAPISIDHASH`, multi-account `X-Goog-AuthUser`, Brand Account `X-Goog-PageId`, logged-out `200 OK` canary detection).
  2. Complete Playlist & Liked Music (`LM` / `VLLM`) Retrieval with full `continuation` pagination + standard YouTube playlist compatibility (`PL...`).
  3. Snapshot Backup, Restore, and Multi-Format Export (`.json`, `.csv`, `.m3u8`).
  4. **Exact & Smart Fuzzy Deduplication** (exact `video_id` + normalized `(artist, title)` Jaro-Winkler + duration guard + quality-tier keeper selection).
  5. **Smart Multi-Key Sorting & Minimal-Move Reordering** (`ACTION_MOVE_VIDEO_BEFORE` using `setVideoId` + `movedSetVideoIdSuccessor` computed via minimal edit/LIS planner + artist-spread smart shuffle).
  6. **Playlist Set Algebra** (`diff`, `intersect`, `merge` with both exact-ID and fuzzy matching modes).
  7. **Playlist Health Check & Upgrade Scanner** (detect `[Deleted video]`, `[Private video]`, unplayable items, and optional `UGC -> Official Audio (ATV)` replacement candidates).
  8. **Metadata Enrichment & Genre Splitting** (SQLite cache -> MusicBrainz -> optional Last.fm / Deezer / ListenBrainz + `genres.toml` taxonomy -> idempotent `split-genre`).
  9. Interactive `ratatui` TUI & `clap` CLI with `comfy-table` dry-run diffs.
- **Tier 2 (Future Backlog — Architecture Must Stay Extensible, Do Not Implement in Initial Release):**
  - Rule-based recurring "Smarter Playlists" DAG/YAML pipeline scheduler.
  - Local HTTP/REST or MCP server daemon for browser extension / [[2_Personal/Claude/AI|AI]] agent integration.
  - *(Keeping `ytm-core`, `ytm-store`, `ytm-ops`, and `ytm-enrich` as pure library crates guarantees Tier 2 can be added later with zero rewrite.)*

---

## 3. OPEN-SOURCE REFERENCE REPOSITORIES (PHASE 0 AUDIT)

Before writing InnerTube payloads or matching heuristics, the **Reference Audit Worker** must inspect these existing open-source projects (cloning into `.refs/` — **must be listed in `.gitignore`**) to extract proven protocol details, edge-case workarounds, and UX lessons:

1. **`sigma67/ytmusicapi`** — `https://github.com/sigma67/ytmusicapi`
   - *Inspect for:* Canonical `youtubei/v1` endpoints (`browse`, `browse/edit_playlist`, `playlist/create`, `playlist/delete`, `search`), `WEB_REMIX` client context fields, `SAPISIDHASH` generation, `X-Goog-AuthUser` / `X-Goog-PageId` handling, `VLLM` (Liked Music) quirks, `continuationCommand` pagination, `setVideoId` extraction, `ACTION_REMOVE_VIDEO`, and `ACTION_MOVE_VIDEO_BEFORE`.
2. **`apastel/ytmusic-deleter`** — `https://github.com/apastel/ytmusic-deleter`
   - *Inspect for:* Multi-attribute playlist sorting strategy, strict vs. fuzzy duplicate matching rules, handling of user-uploaded (`UGC`) vs. official tracks, and safe batch mutation pacing.
3. **`nick42d/youtui` (`ytmapi-rs`)** — `https://github.com/nick42d/youtui`
   - *Inspect for:* Idiomatic Rust async architecture (`tokio` + `reqwest`) over InnerTube, resilient JSON traversal patterns, and strongly typed domain parsing of unstable responses.
4. **`aexzem/ytm-playlist-organizer` (`Dupi`)** — `https://github.com/aexzem/ytm-playlist-organizer`
   - *Inspect for:* Duplicate heuristics across alternate covers/releases, smart tagging UX, and `.md`/`.csv`/`.json` playlist export formats.
5. **`Darren-McConnell/YTMusicDuplicates`** — `https://github.com/Darren-McConnell/YTMusicDuplicates`
   - *Inspect for:* Focused duplicate detection edge cases and `setVideoId` removal behavior on playlists with repeated identical `video_id`s.
6. **`CaullenOmdahl/youtube-music-mcp-server`** — `https://github.com/CaullenOmdahl/youtube-music-mcp-server`
   - *Inspect for:* Clean capability boundaries if `ytm-architect` later exposes an MCP/daemon layer (Tier 2 reference only).

> **Legal & Hygiene Rule:** Study patterns, protocol shapes, and edge cases; write clean, original Rust code licensed under `MIT OR Apache-2.0`. Never copy third-party code verbatim across incompatible licenses, and add `.refs/` to `.gitignore` immediately.

---

## 4. WORKSPACE ARCHITECTURE & CRATE OWNERSHIP

```text
ytm-architect/
├── .gitignore                  # Includes target/, .refs/, *.sqlite, auth.json
├── Cargo.toml                  # Workspace manifest & shared dependency versions
├── LICENSE-MIT / LICENSE-APACHE
├── README.md                   # Cross-platform Windows/Linux setup, auth import, CLI guide
├── config/
│   ├── config.example.toml     # Default clientVersion, rate limits, gl/hl, API keys (env/config)
│   └── genres.example.toml     # Hierarchical subgenre -> parent genre mapping rules
├── crates/
│   ├── ytm-core/               # [Owner: Worker-Core] Auth, InnerTube HTTP + MockTransport, JSON tree parser, pagination, chunked mutations
│   ├── ytm-store/              # [Owner: Worker-Store] ProjectDirs paths, SQLite cache + migrations, atomic snapshots, JSON/CSV/M3U8 export/import
│   ├── ytm-ops/                # [Owner: Worker-Ops] Exact/fuzzy deduper, multi-key sorter + LIS minimal-move planner, set algebra, health scanner
│   ├── ytm-enrich/             # [Owner: Worker-Enrich] Async rate-limited MusicBrainz + optional Last.fm/Deezer/ListenBrainz + genres.toml mapper
│   └── ytm-cli/                # [Owner: Worker-CLI-TUI] Clap CLI, dry-run diff renderer (comfy-table), indicatif progress, Ratatui TUI
└── tests/
    ├── fixtures/               # Sanitized InnerTube JSON response payloads for deterministic tests
    └── integration/            # [Owner: Worker-QA] End-to-end mocked pipeline & CLI --dry-run tests
```

### Approved Dependency Stack
- **Async & HTTP:** `tokio` (`full`), `reqwest` (`default-features = false`, features: `json`, `gzip`, `brotli`, `rustls-tls`)
- **Serialization & Config:** `serde` (`derive`), `serde_json`, `toml`, `csv`
- **Crypto (Auth):** `sha1`, `hex`
- **Persistence & Cross-Platform Paths:** `rusqlite` (`bundled`), `directories`, `chrono` (`serde`)
- **Algorithms & Normalization:** `strsim`, `regex`, `unicode-normalization`, `rand` (for smart shuffle)
- **CLI & TUI:** `clap` (`derive`), `ratatui`, `crossterm`, `comfy-table`, `indicatif`
- **Errors & Observability:** `thiserror` (library crates), `anyhow` (CLI crate), `tracing`, `tracing-subscriber`
- **Trait Async Support:** Native Rust `async fn` in traits (Rust 2024 / 1.75+) or `async-trait` if `dyn Trait` object safety is required across workers. (Define explicitly in Wave 0B!)

---

## 5. CRITICAL TECHNICAL SPECIFICATIONS & INNERTUBE HARDENING

### 5.1. Authentication & The "Logged-Out 200 OK" Trap (`ytm-core`)
1. **Credential Import (`ytm auth import`):**
   - Accept raw HTTP request headers pasted from browser DevTools (`music.youtube.com/youtubei/v1/...`), a `Cookie:` header string, or a `ytmusicapi`-compatible `browser.json`.
   - Persist to `<config_dir>/auth.json` (via `ProjectDirs`). Set `0600` permissions on Unix.
   - Extract and store:
     - `cookie` (containing `SAPISID` and/or `__Secure-3PAPISID`)
     - `user_agent`
     - `x_goog_authuser` (optional, default `"0"` — essential when multiple Google accounts are signed in)
     - `x_goog_page_id` (optional — **essential for YouTube Brand Accounts / secondary channels**)
     - `x_goog_visitor_id` (optional, preserved if present)
2. **Dynamic `SAPISIDHASH` Signing:**
   - Per request: `timestamp = unix_epoch_secs()`, `sapisid = extract_cookie("SAPISID").or(extract_cookie("__Secure-3PAPISID"))`.
   - `digest = hex::encode(Sha1::digest(format!("{timestamp} {sapisid} https://music.youtube.com")))`
   - Header: `Authorization: SAPISIDHASH {timestamp}_{digest}`
   - Required headers: `Origin: https://music.youtube.com`, `X-Origin: https://music.youtube.com`, `Content-Type: application/json`, plus `X-Goog-AuthUser` and `X-Goog-PageId` when configured.
3. **Locale Pinning:**
   - Pin `context.client.hl = "en"` in the InnerTube payload (while allowing configurable `gl`, e.g., `"US"` or `"TR"`) so UI/status strings (`"Liked Music"`, `"[Deleted video]"`, etc.) do not shift unexpectedly with account UI language.
4. **Canary Session Verification (Mandatory!):**
   - InnerTube often returns **HTTP `200 OK` with logged-out/empty payloads** instead of `401`/`403` when cookies expire!
   - Implement `verify_session()` checking an authenticated endpoint (e.g., account menu / library browse `FEmusic_liked_playlists`) for explicit signed-in markers (account header / channel handle / non-logged-out renderer). If absent, fail loudly with `CoreError::AuthExpired`.

### 5.2. Domain Model & Track Identifiers (`ytm-core`)
Every `Track` MUST capture:
- `video_id: String` (media identifier; may be empty/sentinel for certain deleted items)
- `set_video_id: Option<String>` (**mandatory per-entry playlist instance ID** required by `ACTION_REMOVE_VIDEO` and `ACTION_MOVE_VIDEO_BEFORE`)
- `position: usize` (0-based index within the fetched playlist)
- `title: String`
- `artists: Vec<ArtistRef>` (`name`, optional `browse_id`)
- `album: Option<AlbumRef>` (`name`, optional `browse_id`)
- `duration_sec: Option<u32>`
- `release_year: Option<u16>`
- `music_video_type: MusicVideoType` (`AudioTrack` / `ATV`, `OfficialMusicVideo` / `OMV`, `UserGenerated` / `UGC`, `OfficialSourceMusic`, `Unknown`)
- `availability: TrackAvailability` (`Available`, `Deleted`, `Private`, `RegionBlocked`, `Unplayable(String)`)

### 5.3. Pagination, Chunking & Rate Limits
- **Pagination:** All `get_playlist`, `get_liked_songs` (browseId `"VLLM"`), and `get_library_playlists` calls MUST loop over `continuations` / `continuationCommand.token` until exhausted, with a configurable safety ceiling and progress callback.
- **Mutations (`POST /youtubei/v1/browse/edit_playlist`):**
  - Batch `add_playlist_items` and `remove_playlist_items` into chunks of **50 actions max** with a **600ms delay** between requests and exponential backoff with jitter on `429`/`503`.
  - **Reordering (`ACTION_MOVE_VIDEO_BEFORE`):** Each move action sends `setVideoId` and `movedSetVideoIdSuccessor` (`None` / omitted moves item to the end). Because each move alters list order, chunk/execute moves deterministically in the exact order generated by the Minimal-Move Planner.
  - **Mandatory Pre-Mutation Snapshot:** `ytm-core` + `ytm-store` must enforce that no mutating command executes without first persisting a snapshot.

### 5.4. Deduplication & Quality Resolution (`ytm-ops`)
- **Pass 1 (Exact):** Identical `video_id` entries within the same playlist (queued for removal via their distinct `set_video_id`).
- **Pass 2 (Smart Fuzzy):**
  - Normalize `title` and primary `artist`: Unicode NFKC, lowercase, strip bracketed/parenthesized noise (`(Official Video)`, `(Official Music Video)`, `(Official Audio)`, `[Lyrics]`, `(Lyric Video)`, `(Remastered YYYY)`, `(HD)`, `(4K)`, `(Live ...)`, `(feat. ...)` / `ft. ...` extracted cleanly), normalize `&`/`and`/punctuation.
  - Compute `strsim::jaro_winkler` on normalized `(artist, title)` (default threshold `0.90`, configurable).
  - **Duration Guard:** Reject fuzzy match if both tracks have durations differing by more than `--duration-tolerance` (default `15s`) unless `--ignore-duration` is set (prevents falsely merging radio edits with 10-minute extended/live jams).
- **Keeper Priority Scoring (Higher wins):**
  1. `Availability == Available` (+1000)
  2. `MusicVideoType::AudioTrack` (`ATV` / album track) (+100) > `OfficialSourceMusic` (+80) > `OfficialMusicVideo` (`OMV`) (+50) > `UserGenerated` (`UGC`) (+10)
  3. Clean/non-live/non-remaster-noise title (+20)
  4. Earlier playlist position (tie-breaker: keep oldest entry by default, configurable `--keep newest|oldest|highest-quality`).

### 5.5. Smart Multi-Key Sorting & Minimal-Move Reordering (`ytm-ops`)
- Support composable sort keys: `artist`, `album`, `title`, `duration`, `year`, `genre`, `type` (`ATV`/`OMV`/`UGC`), `shuffle` (standard or **artist-spread shuffle** minimizing adjacent tracks by the same artist).
- Support per-key direction (`asc`/`desc`), e.g. `--by artist:asc,year:asc,title:asc`.
- **Minimal-Move Planner (Critical!):**
  - Given current track order $C$ and desired target order $T$ (keyed by `set_video_id`), compute the Longest Increasing Subsequence (LIS) of target indices present in $C$.
  - Items in the LIS remain anchored in place; only items *outside* the LIS are moved using `ACTION_MOVE_VIDEO_BEFORE` (`setVideoId` -> `movedSetVideoIdSuccessor`).
  - Why: Sorting a 500-track playlist that is mostly sorted might require only 40 moves instead of 500, drastically reducing API calls, latency, and rate-limit risk.
  - Unit tests must verify that applying the generated `Vec<MoveAction>` to any permutation $C$ deterministically produces exact order $T$.

### 5.6. Health Check & `UGC -> ATV` Upgrade (`ytm-ops` + `ytm-core`)
- Scan a playlist (or entire library) for:
  - `Deleted`, `Private`, `RegionBlocked`, or `Unplayable` items (exportable or removable).
  - `UGC` / `OMV` tracks where an official `AudioTrack` (`ATV`) equivalent exists via InnerTube search (optional `--find-official` flag, previewed side-by-side in dry-run before swapping).

### 5.7. SQLite Persistence & Multi-Source Enrichment (`ytm-store` + `ytm-enrich`)
- **SQLite Tables (`ytm-store`):**
  - `track_cache(video_id TEXT PRIMARY KEY, title TEXT, artists_json TEXT, album TEXT, duration_sec INTEGER, release_year INTEGER, music_video_type TEXT, updated_at INTEGER)`
  - `artist_genres(artist_name_norm TEXT PRIMARY KEY, raw_tags_json TEXT, primary_genre TEXT, source TEXT, updated_at INTEGER)`
  - `track_enrichment(cache_key TEXT PRIMARY KEY, video_id TEXT, tags_json TEXT, primary_genre TEXT, release_year INTEGER, bpm REAL, source TEXT, updated_at INTEGER)`
  - `snapshots(id TEXT PRIMARY KEY, playlist_id TEXT, playlist_title TEXT, reason TEXT, created_at INTEGER, item_count INTEGER, payload_json TEXT)`
- **Enrichment Pipeline (`ytm-enrich`):**
  1. Check SQLite cache (`track_enrichment` -> `artist_genres`) with configurable TTL (e.g., 30 days).
  2. Query **MusicBrainz** (`/ws/2/recording` & `/ws/2/artist`) strictly paced at `>= 1.1s` per request with a compliant `User-Agent`.
  3. Optional fallback providers (configured via `config.toml` or env vars): **Last.fm** (`track.getTopTags` / `artist.getTopTags`), **Deezer** public catalog search (album release date / genre), **ListenBrainz** metadata.
  4. Apply `genres.toml` hierarchical taxonomy rules (longest/most-specific pattern match -> parent genre, with case-insensitive normalization).

---

## 6. CLI COMMAND SURFACE (`ytm-cli`)

| Command | Purpose | Mutates Remote? |
|---|---|---|
| `ytm auth import [--file <path>]` | Import raw headers / cookies / `browser.json`, run canary check | No |
| `ytm auth status` | Verify active session, show account/Brand Account status | No |
| `ytm list [--json]` | List all user playlists + Liked Music (`LM`) with counts | No |
| `ytm inspect <playlist\|LM> [--json]` | Display tracks, `musicVideoType`, availability, and stats | No |
| `ytm backup <playlist\|LM\|all> [--format json\|csv\|m3u8] [--out <dir>]` | Snapshot to DB + optional file export | No |
| `ytm snapshots list\|show\|restore <snapshot_id> [--execute]` | Manage and restore local snapshots | `restore`: Yes (dry-run default) |
| `ytm dedupe <playlist> [--threshold 0.90] [--duration-tol 15] [--keep quality\|oldest\|newest] [--execute]` | Exact + fuzzy duplicate removal | Yes (dry-run default) |
| `ytm sort <playlist> --by <keys> [--execute]` | Multi-key sort or artist-spread shuffle via minimal `MoveAction`s | Yes (dry-run default) |
| `ytm set <diff\|intersect\|merge> <a> <b> [--fuzzy] [--into <target>] [--execute]` | Set algebra across playlists (`LM` supported as source) | Only with `--into` (dry-run default) |
| `ytm health <playlist\|all> [--find-official] [--prune-unavailable] [--execute]` | Detect dead/private tracks & optionally suggest/swap `UGC -> ATV` | Only with `--prune`/`--upgrade` (dry-run default) |
| `ytm split-genre <playlist\|LM> [--config <genres.toml>] [--min-tracks 5] [--execute]` | Enrich genres & partition into `"{Playlist} - {Genre}"` idempotently | Yes (dry-run default) |
| `ytm tui` | Launch interactive `ratatui` dashboard | Interactive confirm required |

---

## 7. WAVE-BASED WORKER EXECUTION [[2_Personal/ChatGPT/EEE/PLAN|PLAN]] & VERIFICATION GATES

The **Orchestrator** executes this plan strictly wave by wave, delegating to Workers with disjoint file scopes.

### Wave 0A — Reference Audit (`Worker-RefAudit`)
- **Action:** Clone the 6 reference repos from Section 3 into `.refs/` (after creating `.gitignore` ignoring `.refs/`). Inspect `edit_playlist`, `ACTION_MOVE_VIDEO_BEFORE`, `ACTION_REMOVE_VIDEO`, `SAPISIDHASH`, `X-Goog-PageId`, `VLLM`, and duplicate/sort heuristics.
- **Deliverable:** `.refs/RESEARCH_NOTES.md` documenting verified InnerTube request/response schemas and edge cases, plus sanitized mock JSON fixtures under `tests/fixtures/`.
- **Gate 0A:** Orchestrator reviews `.refs/RESEARCH_NOTES.md` and confirms all InnerTube action payloads are concrete.

### Wave 0B — Workspace & Contract Lock (`Worker-Contracts`)
- **Scope:** Root `Cargo.toml`, `.gitignore`, `LICENSE-*`, `config/*.example.toml`, and initial `lib.rs` / `models.rs` / `traits.rs` skeletons across all 5 crates.
- **Deliverables:**
  - All core structs (`Track`, `MusicVideoType`, `TrackAvailability`, `Playlist`, `Snapshot`, `MutationAction`, `MoveAction`, `DedupePlan`, `SortSpec`, `HealthReport`, `DiffPlan`).
  - Core async traits (`InnerTubeClientTrait`, `MetadataProvider`, `SnapshotStore`) enabling clean mocking and parallel crate development.
- **Gate 0B:** `cargo check --workspace` and `cargo test --workspace` pass with 0 warnings.

### Wave 1 — Parallel Core Infrastructure (`Worker-Core` || `Worker-Store`)
- **`Worker-Core` (owns `crates/ytm-core/**`):**
  - Implement `auth.rs` (header/`browser.json`/cookie parser, `SAPISIDHASH`, `X-Goog-AuthUser`, `X-Goog-PageId`), `http.rs` (transport trait + `reqwest` `rustls-tls` client + chunked 50-item pacing + backoff), `parser.rs` (recursive JSON tree extractors, `musicVideoType`, `TrackAvailability`, `setVideoId`), `client.rs` (canary check, `get_library_playlists`, `get_liked_songs`, paginated `get_playlist`, `create_playlist`, `add_playlist_items`, `remove_playlist_items`, `move_playlist_items`, `search_tracks`).
  - Unit tests against `tests/fixtures/` + `SAPISIDHASH` test vectors + logged-out `200 OK` canary test.
- **`Worker-Store` (owns `crates/ytm-store/**`):**
  - Implement `paths.rs` (`ProjectDirs` Windows/Linux paths + Unix `0600` helper), `db.rs` (SQLite schema + migrations + track/genre/enrichment caches), `snapshot.rs` (atomic snapshot save/list/load), `export.rs` (`.json`, `.csv`, `.m3u8` serializers).
  - Integration tests using `:memory:` SQLite and `tempfile` directories.
- **Gate 1:** `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p ytm-core -p ytm-store` pass 100%.

### Wave 2 — Parallel Algorithms & Enrichment (`Worker-Ops` || `Worker-Enrich`)
- **`Worker-Ops` (owns `crates/ytm-ops/**`):**
  - Implement `normalize.rs`, `dedupe.rs` (exact + fuzzy Jaro-Winkler + duration guard + quality scoring), `sort.rs` (multi-key comparator, artist-spread shuffle, **LIS minimal-move planner** generating exact `MoveAction`s), `set_ops.rs` (`diff`, `intersect`, `merge`), `health.rs` (unavailable detector + `UGC -> ATV` candidate matcher).
  - Property/permutation tests verifying minimal-move sorting on random & pathological lists, fuzzy dedupe edge cases, and set algebra invariants.
- **`Worker-Enrich` (owns `crates/ytm-enrich/**`):**
  - Implement `rate_limit.rs`, `musicbrainz.rs` (`1.1s` interval), `fallback.rs` (optional Last.fm / Deezer / ListenBrainz), `taxonomy.rs` (`genres.toml` hierarchical matcher), `splitter.rs` (idempotent `split-genre` planner).
  - Unit tests with mocked HTTP responses verifying cache hits bypass network, rate-limiter pacing, and taxonomy normalization.
- **Gate 2:** `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p ytm-ops -p ytm-enrich` pass 100%.

### Wave 3 — CLI & Interactive TUI (`Worker-CLI-TUI`)
- **Scope:** `crates/ytm-cli/**`
- **Deliverables:**
  - Full `clap` CLI wiring all commands in Section 6.
  - Strict `--dry-run` default enforcement + mandatory pre-mutation snapshot call before any `--execute` remote write.
  - `comfy-table` diff previews, `indicatif` progress bars, and `ratatui` TUI (playlist browser, duplicate side-by-side reviewer, sort move preview, health report view, genre histogram).
- **Gate 3:** `cargo check --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

### Wave 4 — Integration QA, Cross-Platform & Release Verification (`Worker-QA`)
- **Scope:** `tests/integration/**`, `README.md`
- **Deliverables:**
  - End-to-end integration tests wiring `ytm-cli` / `ytm-ops` / `ytm-store` against a mocked `InnerTubeClientTrait` (verifying dry-run vs execute, snapshot creation before mutation, dedupe + sort + restore roundtrip).
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo build --release`
  - Complete `README.md` covering Windows (PowerShell) and Linux setup, browser header extraction, Brand Account configuration, and CLI examples.

---

## 8. HOW TO LAUNCH YOUR ORCHESTRATOR

1. Create a fresh workspace directory and place this file at its root as `ORCHESTRATOR.md` (or `CLAUDE.md`):
   ```bash
   mkdir ytm-architect && cd ytm-architect && git init
   ```
2. Give your Orchestrator Agent this kickoff prompt:
   > `Read ORCHESTRATOR.md completely. Execute Wave 0A (Reference Audit into .refs/, gitignored) using a research worker, summarize the verified InnerTube findings, and then execute Wave 0B (Workspace & Contract Lock). Pass Gate 0B before fanning out Wave 1 workers.`
