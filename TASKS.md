# TASKS.md — ytm-architect Re-Architecture Master Plan

Execution-ready 6-phase (`Phase 0` Contract Lock + `Phases 1–5`) engineering master plan for `ytm-architect` covering all 6 workspace crates (`ytm-core`, `ytm-store`, `ytm-ops`, `ytm-enrich`, `ytm-cli`, `ytm-gui`). Designed for sequential or parallel worktree execution with strict non-overlapping file ownership per wave.

---

## 0. Architectural Decisions (Locked)

All 5 architectural decisions are **LOCKED** across the 6-crate workspace (`ytm-core`, `ytm-store`, `ytm-ops`, `ytm-enrich`, `ytm-cli`, `ytm-gui`):

1. **D1 — Mutation Governance (`MutationPolicy`):** All playlist/library writes (`AddTracks`, `RemoveTracks`, `MoveTracks`, `Reorder`, `CreatePlaylist`, `UpdatePlaylistMeta`, `DeletePlaylist`, `UpgradeTracks`, `RestoreSnapshot`) are strictly locked behind `SyncQueue → Preview → Verified POST`. `SetLikeStatus` (`♥`) defaults to `MutationPolicy::StrictLock` (staged in `SyncQueue`) with an opt-in `MutationPolicy::QuickLike` Settings toggle (immediate POST + failure rollback + `LM` cache sync).
2. **D2 — Fetch Completeness Guard (`FetchCompleteness`):** Tiered Guard — `Truncated` (pagination ceiling/interruption) and `Degraded` (dropped/unparsed renderers) hard-lock `Sort`, `Dedupe`, `Health`, and `Sync` dispatch until cleanly re-fetched; `PartialHidden` (YouTube hidden/deleted videos) shows an amber warning badge and detail popover while allowing explicitly confirmed edits.
3. **D3 — Startup Browser Cookie Refresh & Cross-Platform IPC Scope:** Silent startup `yt-dlp --cookies-from-browser` extraction is **OFF** (`auto_refresh_browser_cookies = false`); expired sessions render an actionable Auth banner + 1-click refresh. Cross-platform support targets both Linux (Unix socket + MPRIS2 + `secret-tool`) and Windows (Named Pipe `mpv` IPC + SMTC + OS credential store).
4. **D4 — 6-Crate Workspace Topology & Enrichment Lookup Strategy:** Keep the existing **6-crate workspace** (no 7th crate). Shared pure diff/restore planning (`plan_restore_diff`) lives in `ytm-ops`; shared persistence (`staged_ops`, `CachedPlaylistRecord`, `playback_session`) lives in `ytm-store`. `EnrichmentConfig` adds opt-in `artist_cache_first = false` (`false` = recording-level accuracy, `true` = fast large-playlist splits).
5. **D5 — Phase 5 UI Architecture & Post-Sync Verification:** Consolidate 11 flat tabs into **5 Workspaces** (`Home & Explore`, `Library & Playlist`, `Studio Tools`, `Safety & History`, `Settings & Auth`) + a toggleable **Sync Staging Drawer** reachable from every screen. Sync dispatch executes dual-layer proof: pre-flight live drift check + chunk `STATUS_SUCCEEDED` verification + post-mutation read-back `FetchManifest`.

---

## 0.5. Locked Domain Contracts & State Machines (Merged from Domain Model)

Make illegal states unrepresentable across all 6 crates (zero parallel boolean flags):

```rust
// ytm-core: replaces `owned: bool` + `is_liked_music: bool` + ad-hoc `"LM"` checks
pub enum PlaylistAccess { EditableOwned, LikedMusicAuto, ReadOnlyExternal }

// ytm-core: Tiered Fetch Guard driving UI badges & mutation locks
pub enum FetchCompleteness {
    Complete,
    PartialHidden { declared: usize, fetched: usize, hidden_estimate: usize },
    Truncated { pages_fetched: usize, max_pages: usize },       // HARD-LOCK mutations
    Degraded { dropped_renderers: Vec<String> },                // HARD-LOCK mutations
}

pub struct FetchManifest {
    pub declared_track_count: Option<usize>,
    pub fetched_tracks: usize,
    pub available_tracks: usize,
    pub unavailable_tracks: usize,
    pub missing_set_video_id: usize,
    pub pages_fetched: usize,
    pub completeness: FetchCompleteness,
    pub fetched_at: DateTime<Utc>,
}

// ytm-gui: closed remote-write set & explicit session/dispatch state machines
pub enum MutationPolicy { StrictLock, QuickLike } // default: StrictLock
pub enum SessionState {
    Unauthenticated,
    Verifying,
    Authenticated(SessionStatus),
    Expired { reason: String, last_known: Option<SessionStatus> },
}
pub enum StagedOp {
    AddTracks { playlist_id: String, playlist_title: String, tracks: Vec<Track>, allow_duplicates: bool },
    RemoveTracks { playlist_id: String, playlist_title: String, tracks: Vec<Track> },
    MoveTracks { source_playlist_id: String, source_playlist_title: String, target_playlist_id: String, target_playlist_title: String, tracks: Vec<Track> },
    Reorder { playlist_id: String, playlist_title: String, target_set_video_ids: Vec<String> },
    CreatePlaylist { title: String, description: String, privacy: PlaylistPrivacy, tracks: Vec<Track> },
    UpdatePlaylistMeta { playlist_id: String, playlist_title: String, title: Option<String>, description: Option<String>, privacy: Option<PlaylistPrivacy> },
    DeletePlaylist { playlist_id: String, playlist_title: String },
    UpgradeTracks { playlist_id: String, playlist_title: String, upgrades: Vec<(Track, Track)>, preserve_position: bool },
    RestoreSnapshot { snapshot_id: String, playlist_id: String, playlist_title: String },
    SetLikeStatus { video_id: String, title: String, liked: bool },
}
pub enum DispatchEntryOutcome {
    Succeeded { entry_id: u64, chunks_sent: usize, confirmed_items: usize, readback_verified: bool },
    AbortedDrift { entry_id: u64, reason: String },
    Failed { entry_id: u64, completed_chunks: usize, total_chunks: usize, error: String },
}
```

```text
6-Crate Dependency Flow (Locked):
ytm-gui (eframe/egui + mpv IPC + MPRIS2/SMTC) ──┐
ytm-cli (Clap CLI + Ratatui TUI)               ──┼──> ytm-ops (LIS, Dedupe, SetOps, Health, RestoreDiff)
                                                 ├──> ytm-enrich (MusicBrainz/Last.fm/Deezer + Taxonomy)
                                                 ├──> ytm-store (SQLite: Cache+Manifest, Snapshots, StagedOps, PlaybackSession)
                                                 └──> ytm-core (InnerTube HTTP/2, SAPISIDHASH, Tree Parser, FetchManifest)
```

---

## 1. Phase-to-Skill & Parallelism Matrix

| Phase | Focus | Primary Skills | Execution Mode | Exit Gate |
| :--- | :--- | :--- | :--- | :--- |
| **Phase 0** | Contract, Spec & Domain Model Lock | `domain-modeling`, `to-spec`, `writing-plans`, `verification-before-completion` | Wave 0.1 Parallel Worktrees → Wave 0.2 Sequential | 4 Cargo Gates + `PROGRESS.md` |
| **Phase 1** | Mutation Lock, Persistent Staging & `Configure → Preview → POST` | `test-driven-development`, `surgical-patch`, `migration`, `using-git-worktrees`, `dispatching-parallel-agents`, `verification-before-completion` | Waves 1.1–1.2 Parallel Worktrees → Wave 1.3 Sequential | 4 Cargo Gates + `PROGRESS.md` |
| **Phase 2** | `FetchManifest`, Pagination Proof, Parser & POST Verification | `test-driven-development`, `surgical-patch`, `domain-modeling`, `using-git-worktrees`, `dispatching-parallel-agents`, `verification-before-completion` | Wave 2.1 Parallel → Wave 2.2 Sequential → Wave 2.3 Parallel | 4 Cargo Gates + `PROGRESS.md` |
| **Phase 3** | 3-Lane Async Worker, 60 FPS Memoization & Cross-Platform Core | `safe-refactor`, `karpathy-guidelines`, `lean-build`, `using-git-worktrees`, `subagent-driven-development`, `verification-before-completion` | Waves 3.1–3.3 Parallel Worktrees | 4 Cargo Gates + `PROGRESS.md` |
| **Phase 4** | Daily-Driver YTM: Gapless `mpv`, MPRIS2/SMTC, Search/Explore, Settings | `lean-build`, `test-driven-development`, `migration`, `using-git-worktrees`, `dispatching-parallel-agents`, `verification-before-completion` | Wave 4.1 Sequential → Waves 4.2–4.3 Parallel Worktrees | 4 Cargo Gates + `PROGRESS.md` |
| **Phase 5** | 5-Workspace IA, Sync Drawer, Review & Dispatch UI & Visual Polish | `hallmark`, `brainstorming`, `safe-refactor`, `karpathy-guidelines`, `requesting-code-review`, `finishing-a-development-branch` | Wave 5.1 Parallel → Wave 5.2 Sequential → Wave 5.3 Parallel | 4 Cargo Gates + Release Build + `PROGRESS.md` |

---

## 2. Master Task Breakdown

### Phase 0: Contract, Spec & Domain Model Lock

#### Wave 0.1: Documentation Alignment & Glossary (Parallel-Safe)

- [x] **P0-W1-T1: Author Canonical `GLOSSARY.md` Terminology Registry**
  - **Skills:** `domain-modeling`, `to-spec`
  - **Owns:** `GLOSSARY.md`
  - **Depends On:** None
  - **Subtasks:**
    - [x] P0-W1-T1.1: Define single-line canonical terms (`PlaylistAccess`, `FetchManifest`, `FetchCompleteness`, `MutationPolicy`, `StagedOp`, `StagedEntry`, `DraftRevision`, `PlanFingerprint`, `DispatchPlan`, `DispatchReport`, `WorkerLane`, `SessionState`).
    - [x] P0-W1-T1.2: Document forbidden competing synonyms (e.g., `owned && !is_liked_music` boolean pairs replaced by `PlaylistAccess`; `pending` vs `queued` standardized to `StagedEntry`).
  - **Verify:** `test -s GLOSSARY.md && grep -q "FetchManifest" GLOSSARY.md && grep -q "MutationPolicy" GLOSSARY.md`
  - **DoD:** `GLOSSARY.md` exists at repo root in English with all 12 canonical domain terms locked.

- [x] **P0-W1-T2: Reconcile `ARCHITECTURE.md` & `SPEC.md` with Live 6-Crate Codebase**
  - **Skills:** `to-spec`, `domain-modeling`
  - **Owns:** `ARCHITECTURE.md`, `SPEC.md`
  - **Depends On:** None
  - **Subtasks:**
    - [x] P0-W1-T2.1: Update `ARCHITECTURE.md` from stale 5-crate tree to live 6-crate workspace (`ytm-gui` + `mpv` IPC + `ArtworkCache` + 3-lane worker topology).
    - [x] P0-W1-T2.2: Reconcile `SPEC.md` discrepancies against live code: add `Track.thumbnail_url`, `browse_models.rs`, `playlist_cache` & `staged_ops` tables, `MutationAction::{SetPlaylistName,SetPlaylistDescription,SetPlaylistPrivacy}`, and resolve `SPEC.md` §3 Keeper Priority Scoring (`+1000/+100`) vs live `crates/ytm-ops/src/dedupe.rs` (`+50/-100/+40`) weights.
    - [x] P0-W1-T2.3: Codify `Configure → Preview → Verified POST`, `FetchManifest`, and `MutationBatchReport` wire invariants in `SPEC.md`.
  - **Verify:** `grep -q "ytm-gui" ARCHITECTURE.md && grep -q "FetchManifest" SPEC.md && grep -q "playlist_cache" SPEC.md`
  - **DoD:** `ARCHITECTURE.md` and `SPEC.md` match live code with zero stale crate counts or undocumented tables/weights.

#### Wave 0.2: Core, Store & GUI State Machine Contract Lock (Sequential)

- [x] **P0-W2-T1: Lock `ytm-core` Domain & Telemetry Contracts**
  - **Skills:** `domain-modeling`, `test-driven-development`
  - **Owns:** `crates/ytm-core/src/models.rs`, `crates/ytm-core/src/traits.rs`, `crates/ytm-core/src/error.rs`, `crates/ytm-core/src/lib.rs`
  - **Depends On:** `P0-W1-T1`, `P0-W1-T2`
  - **Subtasks:**
    - [x] P0-W2-T1.1: Add `PlaylistAccess` (`EditableOwned`, `LikedMusicAuto`, `ReadOnlyExternal`) with helper `access()` on `Playlist` & `PlaylistSummary` (keeping serde backwards compatibility for existing snapshots).
    - [x] P0-W2-T1.2: Define `FetchCompleteness` (`Complete`, `PartialHidden`, `Truncated`, `Degraded`), `FetchManifest`, `PlaylistFetchResult`, `ChunkOutcome`, and `MutationBatchReport`.
    - [x] P0-W2-T1.3: Add unit tests validating serde roundtrip for `FetchManifest`, `PlaylistAccess`, and backwards deserialization of legacy snapshot JSON lacking new fields.
  - **Verify:** `cargo test -p ytm-core --lib`
  - **DoD:** All new `ytm-core` domain types compile cleanly, re-export via `lib.rs`, and pass unit serde roundtrip tests.

- [x] **P0-W2-T2: Lock `ytm-store` Persistence Contracts (`staged_ops`, `CachedPlaylistRecord`, `playback_session`)**
  - **Skills:** `domain-modeling`, `migration`
  - **Owns:** `crates/ytm-store/src/models.rs`, `crates/ytm-store/src/traits.rs`, `crates/ytm-store/src/db.rs`, `crates/ytm-store/src/lib.rs`
  - **Depends On:** `P0-W2-T1`
  - **Subtasks:**
    - [x] P0-W2-T2.1: Add idempotent SQLite schema migrations for `staged_ops`, `playback_session`, and `manifest_json` column on `playlist_cache` (via `ALTER TABLE` guard preserving existing rows).
    - [x] P0-W2-T2.2: Define `CachedPlaylistRecord { playlist, manifest, fetched_at }`, `PersistedStagedOp`, and `PersistedPlaybackSession` models.
    - [x] P0-W2-T2.3: Add `:memory:` migration test verifying upgrade from v1 schema (without `manifest_json` / `staged_ops`) preserves existing snapshots and playlist cache rows.
  - **Verify:** `cargo test -p ytm-store migration`
  - **DoD:** Schema migrations run idempotently on both fresh and pre-existing SQLite databases with zero data loss.

- [x] **P0-W2-T3: Lock `ytm-gui` Mutation Policy, Closed `StagedOp` & Session State Machines**
  - **Skills:** `domain-modeling`, `writing-plans`
  - **Owns:** `crates/ytm-gui/src/state.rs`, `crates/ytm-gui/src/worker_msgs.rs`, `crates/ytm-gui/src/sync_queue.rs`
  - **Depends On:** `P0-W2-T1`, `P0-W2-T2`
  - **Subtasks:**
    - [x] P0-W2-T3.1: Add `MutationPolicy` (`StrictLock` default, `QuickLike`), `SessionState` (`Unauthenticated`, `Verifying`, `Authenticated`, `Expired`), `DraftRevision(u64)`, `PlanFingerprint`, and `JobId` / `CancelJob(JobId)` commands/events.
    - [x] P0-W2-T3.2: Extend `StagedOp` to the closed set of all remote writes: add atomic `MoveTracks`, sparse `UpdatePlaylistMeta`, `DeletePlaylist`, `RestoreSnapshot`, `SetLikeStatus`, and `preserve_position: bool` on `UpgradeTracks`; add `selected: bool` on `StagedEntry`.
    - [x] P0-W2-T3.3: Add `GuiAppConfig.mutation_policy` and `GuiAppConfig.auto_refresh_browser_cookies` (`false` default) with `#[serde(default)]` backwards compatibility.
  - **Verify:** `cargo check --workspace --all-targets`
  - **DoD:** Workspace compiles with zero warnings under the new locked state machines and closed `StagedOp` contract.

#### Phase 0 Exit Gate
```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- Update `PROGRESS.md` with Phase 0 gate evidence.

---

### Phase 1: Mutation Governance, Strict Staging Lock & Safe `Configure → Preview → POST` Pipeline

#### Wave 1.1: Persistent `SyncQueue` & Atomic Draft Engine (Parallel-Safe)

- [x] **P1-W1-T1: Implement SQLite `staged_ops` Persistence & Crash Recovery**
  - **Skills:** `test-driven-development`, `migration`
  - **Owns:** `crates/ytm-store/src/staged_ops.rs`, `crates/ytm-store/src/lib.rs`, `crates/ytm-store/tests/store_integration_test.rs`
  - **Depends On:** `P0-W2-T2`
  - **Subtasks:**
    - [x] P1-W1-T1.1: Implement `replace_staged_ops`, `load_staged_ops`, `delete_staged_op`, and `clear_staged_ops` inside a single SQLite transaction.
    - [x] P1-W1-T1.2: Write failing-first integration test proving `staged_ops` roundtrip preserves execution ordering, `entry_id`, and `selected` flags across reopen.
  - **Verify:** `cargo test -p ytm-store staged_ops`
  - **DoD:** Staged operations survive process restarts and crashes atomically in SQLite.

- [x] **P1-W1-T2: Upgrade `SyncQueue` Semantics (Atomic `MoveTracks`, `DraftRevision`, Dependency-Safe Reordering)**
  - **Skills:** `test-driven-development`, `domain-modeling`
  - **Owns:** `crates/ytm-gui/src/sync_queue.rs`, `crates/ytm-gui/src/sync_queue_tests.rs`
  - **Depends On:** `P0-W2-T3`
  - **Subtasks:**
    - [x] P1-W1-T2.1: Implement working-draft projection for `MoveTracks`, `UpdatePlaylistMeta`, `DeletePlaylist`, `SetLikeStatus`, and `RestoreSnapshot`; increment `DraftRevision` on every mutation.
    - [x] P1-W1-T2.2: Add `remove_entries(&[u64])` (selective completion pruning), `selected_entries()`, and dependency validation in `move_entry` (prevent moving a `Reorder` before a structural edit on the same `playlist_id` without recompute warning).
    - [x] P1-W1-T2.3: Add unit tests for atomic `MoveTracks` single-step Undo, sparse `UpdatePlaylistMeta` draft overlay, and `remove_entries` preserving unexecuted items.
  - **Verify:** `cargo test -p ytm-gui sync_queue`
  - **DoD:** All new `StagedOp` variants project accurately onto local drafts and pass unit tests for single-unit Undo and selective pruning.

#### Wave 1.2: Eliminate All Direct-POST Bypasses & Ordering Bugs (Parallel-Safe)

- [x] **P1-W2-T1: Route `PlaylistMetaModal` into `SyncQueue` & Prevent Uncached Overwrite**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/playlist_meta_modal.rs`, `crates/ytm-gui/src/sidebar.rs`
  - **Depends On:** `P1-W1-T2`
  - **Subtasks:**
    - [x] P1-W2-T1.1: In `sidebar.rs`, block opening `PlaylistMetaModal` on uncached playlists until `LoadPlaylist` hydrates real `description` and `privacy` (never default to `""` / `Private`).
    - [x] P1-W2-T1.2: In `playlist_meta_modal.rs`, compute sparse diff (`title`, `description`, `privacy` as `Option` only when changed) and push `StagedOp::UpdatePlaylistMeta` into `SyncQueue` instead of sending `WorkerCmd::UpdatePlaylistMeta`.
    - [x] P1-W2-T1.3: Add unit test verifying renaming a playlist emits `description: None, privacy: None`.
  - **Verify:** `cargo test -p ytm-gui playlist_meta`
  - **DoD:** Zero direct metadata POSTs remain; uncached playlists cannot have description/privacy silently wiped.

- [x] **P1-W2-T2: Fix Reversed Move Data-Loss Order, Split Undo & Bare `Enter` Modal Confirm**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/views/repertoire_batch.rs`, `crates/ytm-gui/src/overlays.rs`
  - **Depends On:** `P1-W1-T2`
  - **Subtasks:**
    - [x] P1-W2-T2.1: Replace reversed `RemoveTracks`-then-`AddTracks` in `repertoire_batch.rs` and two-entry move in `overlays.rs` with a single atomic `StagedOp::MoveTracks` entry.
    - [x] P1-W2-T2.2: Remove bare `Key::Enter` auto-confirmation in `overlays.rs::show_confirmation_modal`; require explicit button click, and require typed playlist title match when confirming `DeletePlaylist`.
    - [x] P1-W2-T2.3: Add unit test verifying `stage_move_tracks` creates exactly 1 `StagedEntry` and `Undo` reverts both source and target drafts cleanly.
  - **Verify:** `cargo test -p ytm-gui overlays`
  - **DoD:** No move action can orphan tracks or half-undo; pressing `Enter` never accidentally dispatches remote POSTs.

- [x] **P1-W2-T3: Lock `SetLikeStatus` Under `MutationPolicy` & Remove Direct Meta Handler**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/playback_ops.rs`, `crates/ytm-gui/src/worker_media.rs`
  - **Depends On:** `P1-W1-T2`
  - **Subtasks:**
    - [x] P1-W2-T3.1: In `playback_ops.rs`, route `PlayAction::ToggleLike` to `StagedOp::SetLikeStatus` when `MutationPolicy::StrictLock` is active; when `QuickLike` is active, send `SetLikeStatus` with explicit rollback event `LikeStatusResult { video_id, liked, result }` and `LM` cache sync.
    - [x] P1-W2-T3.2: Remove immediate `WorkerCmd::UpdatePlaylistMeta` execution path from `worker_media.rs`.
    - [x] P1-W2-T3.3: Add unit test asserting `StrictLock` stages `SetLikeStatus` without emitting `WorkerCmd::SetLikeStatus`, while `QuickLike` rolls back `liked_video_ids` on error.
  - **Verify:** `cargo test -p ytm-gui playback_ops`
  - **DoD:** `SetLikeStatus` obeys `MutationPolicy` with rollback safety; zero direct metadata mutations remain in `worker_media.rs`.

- [x] **P1-W2-T4: Opt-In Startup Browser Refresh, Bootstrap Hang Fix & Non-Disruptive Quality Switch**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/worker_auth.rs`, `crates/ytm-gui/src/views/auth.rs`, `crates/ytm-gui/src/player.rs`
  - **Depends On:** `P0-W2-T3`
  - **Subtasks:**
    - [x] P1-W2-T4.1: In `worker_auth.rs::handle_bootstrap`, gate `import_browser_auth_inner` behind `state.config.auto_refresh_browser_cookies` (`false` default) and ALWAYS emit `SessionLoaded(Err(...))` when verification fails (fixing the infinite `Loading` hang when `state.client.is_some()`).
    - [x] P1-W2-T4.2: In `player.rs`, defer `ActorCmd::Configure` `mpv` restart while actively playing (apply on next track or resume at saved `position_sec` only on explicit "Apply Now").
    - [x] P1-W2-T4.3: In `views/auth.rs`, display an explicit error toast when `Import from File...` fails to read the given path instead of failing silently.
  - **Verify:** `cargo test -p ytm-gui worker_auth`
  - **DoD:** Startup never runs `yt-dlp` silently by default, never hangs in `Loading`, and changing audio quality never abruptly drops active playback.

#### Wave 1.3: Verified Sync Executor, Selective Pruning & Pre-Flight POST Inspector (Sequential)

- [x] **P1-W3-T1: Harden `execute_sync_queue` with Mandatory Snapshots, Live Drift Check & Per-Entry Accounting**
  - **Skills:** `test-driven-development`, `surgical-patch`
  - **Owns:** `crates/ytm-gui/src/worker_exec.rs`, `crates/ytm-gui/src/worker.rs`
  - **Depends On:** `P1-W1-T1`, `P1-W2-T1`, `P1-W2-T2`, `P1-W2-T3`, `P1-W2-T4`
  - **Subtasks:**
    - [x] P1-W3-T1.1: Make pre-delete/pre-sync/pre-meta/pre-restore SQLite snapshots strictly mandatory (`?` propagation; abort before any POST if snapshot fails).
    - [x] P1-W3-T1.2: Implement live execution for `MoveTracks` (Add → verify `added_items` non-empty → Remove), `UpdatePlaylistMeta`, `DeletePlaylist`, `RestoreSnapshot`, `SetLikeStatus`, and live `Reorder` LIS recomputation against fetched `live` `setVideoId`s (aborting entry cleanly on drift).
    - [x] P1-W3-T1.3: Emit `WorkerEvent::SyncEntryFinished { entry_id, outcome }` after each entry and persist updated `staged_ops` in `ytm-store` so a failure on entry 4/5 NEVER re-runs entries 1–3.
    - [x] P1-W3-T1.4: Write mocked `InnerTubeClientTrait` tests covering partial sync failure (3 pass, 4 fails → only entries 1–3 pruned), `DeletePlaylist` snapshot-failure abort, and `Reorder` drift abort.
  - **Verify:** `cargo test -p ytm-gui worker_exec`
  - **DoD:** Every remote write passes through `execute_sync_queue` with mandatory snapshot, drift check, and zero duplicate re-execution on partial failure.

- [x] **P1-W3-T2: Wire Selective Queue Pruning, Startup Draft Hydration & Stale-Plan Clearing**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/app_events.rs`, `crates/ytm-gui/src/app.rs`
  - **Depends On:** `P1-W3-T1`
  - **Subtasks:**
    - [x] P1-W3-T2.1: Update `app_events.rs` to prune ONLY `completed_entry_ids` (removing blanket `app.sync_queue.clear()`), hydrate persisted `staged_ops` on `Bootstrap`, and handle `LikeStatusResult` rollback.
    - [x] P1-W3-T2.2: Clear `dedupe_plan`, `sort_plan`, `health_report`, and `genre_plan` on `selected_playlist_id` change in `app.rs`; trigger SQLite `staged_ops` persistence on every `DraftRevision` bump.
    - [x] P1-W3-T2.3: Add unit test proving `MutationFinished` for entry `#1` leaves entry `#2` intact in `app.sync_queue`.
  - **Verify:** `cargo test -p ytm-gui app::tests`
  - **DoD:** Completing a single sync entry or meta edit never wipes unrelated staged ops; staged drafts persist across app restarts.

- [x] **P1-W3-T3: Build Pre-Flight POST Inspector & Enforce `PlanFingerprint` + `PlaylistAccess` Locks**
  - **Skills:** `surgical-patch`, `domain-modeling`, `karpathy-guidelines`
  - **Owns:** `crates/ytm-gui/src/views/sync.rs`, `crates/ytm-gui/src/views/dedupe.rs`, `crates/ytm-gui/src/views/sort.rs`, `crates/ytm-gui/src/views/set_ops.rs`, `crates/ytm-gui/src/views/health.rs`, `crates/ytm-gui/src/views/genres_snapshots.rs`
  - **Depends On:** `P1-W3-T2`
  - **Subtasks:**
    - [x] P1-W3-T3.1: Add Pre-Flight POST preview to `views/sync.rs`: per-entry checkbox (`selected`), expandable endpoint/chunk/action breakdown (showing exact track IDs & `POST browse/edit_playlist` chunk count), and "Sync Selected" button.
    - [x] P1-W3-T3.2: Enforce `PlanFingerprint` staleness lock across `dedupe.rs`, `sort.rs`, `set_ops.rs`, `health.rs`, and `genres_snapshots.rs` (disable `Stage` button + show "Parameters changed — re-scan" whenever form or playlist changes).
    - [x] P1-W3-T3.3: Gate all mutating `Stage` buttons with `PlaylistAccess::EditableOwned` (disabling mutations on `LM` and external read-only playlists with clear tooltip); route `Restore Snapshot` through `StagedOp::RestoreSnapshot`.
  - **Verify:** `cargo test -p ytm-gui`
  - **DoD:** Every tool tab enforces `Configure → Preview (fingerprinted) → Stage → POST Inspector → Confirmed Sync` with zero read-only/`LM` leaks.

#### Phase 1 Exit Gate
```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- Update `PROGRESS.md` with Phase 1 gate evidence.

---

### Phase 2: Fetch Completeness, Pagination Proof & POST Response Verification

#### Wave 2.1: HTTP Transport & Parser Hardening (Parallel-Safe)

- [x] **P2-W1-T1: Harden `ReqwestTransport` HTTP/2 Headroom, Retryable API Key & Safe Retry Policy**
  - **Skills:** `test-driven-development`, `surgical-patch`
  - **Owns:** `crates/ytm-core/src/http.rs`, `crates/ytm-core/src/auth.rs`
  - **Depends On:** `P0-W2-T1`
  - **Subtasks:**
    - [x] P2-W1-T1.1: Configure `.http2_max_header_list_size(64 * 1024)` in `ReqwestTransport::new` (enforcing `MEMORY.md` Trap 10).
    - [x] P2-W1-T1.2: Replace `OnceCell<Option<String>>` with `tokio::sync::RwLock<Option<String>>` so transient startup offline failures do NOT permanently cache `None` for `INNERTUBE_API_KEY`.
    - [x] P2-W1-T1.3: Add jittered backoff (`±25%`), refresh `SAPISIDHASH` timestamp on each retry, and add `RetryPolicy::IdempotentRead` vs `RetryPolicy::MutationSafe` (never blindly retry ambiguous timeout/`503` on non-idempotent POSTs; support `409`/`400` post-create backoff explicitly).
    - [x] P2-W1-T1.4: Add unit tests for retryable key recovery after failure and jitter bounds.
  - **Verify:** `cargo test -p ytm-core http`
  - **DoD:** HTTP/2 64 KiB header cap is active, API key recovers after transient network failure, and mutations never blind-retry on unknown outcomes.

- [x] **P2-W1-T2: Harden `parser/playlist.rs` Pagination, Header Count & Strict POST Response Checks**
  - **Skills:** `test-driven-development`, `surgical-patch`
  - **Owns:** `crates/ytm-core/src/parser/playlist.rs`, `crates/ytm-core/src/parser/tree.rs`
  - **Depends On:** `P0-W2-T1`
  - **Subtasks:**
    - [x] P2-W1-T2.1: Update `parse_library_playlists` to return `(Vec<PlaylistSummary>, Option<String>)` extracting `gridRenderer` continuation tokens and checking `is_explicitly_logged_out`.
    - [x] P2-W1-T2.2: Extract header `declared_track_count` in `parse_playlist_page` (from `musicResponsiveHeaderRenderer.secondSubtitle` / `collapsedItemCount`) instead of discarding it.
    - [x] P2-W1-T2.3: Fix `extract_continuation_token` to scan ALL `items` (not just `items.last()`) and support `reloadContinuationItemsCommand` so trailing `messageRenderer` ("Unavailable videos are hidden") never truncates pagination.
    - [x] P2-W1-T2.4: Tighten `parse_edit_playlist_response` to reject any status other than `STATUS_SUCCEEDED` (unless endpoint is `playlist/delete`) and extract `addToToastAction` error strings into `CoreError::ApiError`.
  - **Verify:** `cargo test -p ytm-core parser::playlist`
  - **DoD:** Library pagination, non-last continuation tokens, declared header counts, and strict `STATUS_SUCCEEDED` checks pass fixture tests.

- [x] **P2-W1-T3: Locale-Independent Track Availability & Dropped-Renderer Telemetry**
  - **Skills:** `test-driven-development`, `surgical-patch`
  - **Owns:** `crates/ytm-core/src/parser/track.rs`
  - **Depends On:** `P0-W2-T1`
  - **Subtasks:**
    - [x] P2-W1-T3.1: Return `(Vec<Track>, Vec<String>)` from `parse_tracks_from_items` recording every unrecognized/dropped renderer key (excluding `continuationItemRenderer` & `messageRenderer`) so nothing disappears silently.
    - [x] P2-W1-T3.2: Make `classify_availability` locale-independent: mark tracks missing both `watchEndpoint` and `playNavigationEndpoint` (or with `GREY_OUT` policy) as `Unplayable`, in addition to localized deleted/private patterns (`tr`/`en`).
    - [x] P2-W1-T3.3: Clean fallback artist extraction in `parse_columns` to split `•`-separated runs and strip view-count/duration tokens instead of storing the entire column as one artist.
  - **Verify:** `cargo test -p ytm-core parser::track`
  - **DoD:** Non-English unavailable tracks are accurately classified and unknown renderers surface in `FetchManifest.dropped_renderers`.

#### Wave 2.2: `InnerTubeClient` Full Pagination, `FetchManifest` & Chunked Mutations (Sequential)

- [x] **P2-W2-T1: Wire Full Pagination, `FetchManifest`, `MutationBatchReport` & Chunked `create_playlist`**
  - **Skills:** `test-driven-development`, `domain-modeling`
  - **Owns:** `crates/ytm-core/src/client.rs`, `crates/ytm-core/tests/fixtures_and_client_test.rs`, `crates/ytm-core/tests/browse_and_mutations_test.rs`, `tests/fixtures/`
  - **Depends On:** `P2-W1-T1`, `P2-W1-T2`, `P2-W1-T3`
  - **Subtasks:**
    - [x] P2-W2-T1.1: Loop continuation tokens in `get_library_playlists` with `seen_tokens: HashSet<String>` loop guard and `max_pagination_pages` safety cap.
    - [x] P2-W2-T1.2: Implement `get_playlist_with_manifest` computing `FetchManifest` (`declared_track_count`, `fetched_tracks`, `available_tracks`, `unavailable_tracks`, `missing_set_video_id`, `dropped_renderers`, `pages_fetched`, `completeness`); remove early break on empty intermediate pages when a fresh `next_token` exists.
    - [x] P2-W2-T1.3: Return full `MutationBatchReport` from `send_edit_chunks` preserving every chunk's outcome even if chunk `k` fails (never overwriting prior statuses or dropping `.last()`).
    - [x] P2-W2-T1.4: Chunk `create_playlist` when `video_ids.len() > mutation_chunk_size`: create with initial chunk (or empty), then append remaining chunks via `add_playlist_items` with `409`/`400` backoff retry.
    - [x] P2-W2-T1.5: Add sanitized JSON fixtures and integration tests for multi-page library, trailing `messageRenderer` continuation, `PartialHidden` vs `Truncated` vs `Degraded` manifests, and chunked `create_playlist` with `409` recovery.
  - **Verify:** `cargo test -p ytm-core`
  - **DoD:** Every fetch produces a verified `FetchManifest`, library lists paginate completely, and chunked POSTs never lose partial status or fail on 200+ track creates.

#### Wave 2.3: Store Staleness, Position-Preserving Upgrades & Tiered Guard Enforcement (Parallel-Safe)

- [x] **P2-W3-T1: Persist `FetchManifest` & Staleness in `playlist_cache` + Prune Deleted Orphans**
  - **Skills:** `test-driven-development`, `migration`
  - **Owns:** `crates/ytm-store/src/playlist_cache.rs`, `crates/ytm-store/tests/store_integration_test.rs`
  - **Depends On:** `P2-W2-T1`
  - **Subtasks:**
    - [x] P2-W3-T1.1: Store and return `CachedPlaylistRecord { playlist, manifest, fetched_at }` in `upsert_cached_playlist` and `load_all_cached_playlists`.
    - [x] P2-W3-T1.2: Add `prune_missing_cached_playlists(&[String])` to delete orphaned playlists removed on YouTube Music.
    - [x] P2-W3-T1.3: Add roundtrip and orphan-pruning integration tests.
  - **Verify:** `cargo test -p ytm-store playlist_cache`
  - **DoD:** Every cached playlist carries its `FetchManifest` and `fetched_at` timestamp; deleted remote playlists are pruned from SQLite.

- [x] **P2-W3-T2: Enforce Tiered `FetchCompleteness` Guard, Read-Back Proof & Verified `UpgradeTracks` in GUI**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/worker_exec.rs`, `crates/ytm-gui/src/app_events.rs`, `crates/ytm-gui/src/views/inspect.rs`
  - **Depends On:** `P2-W2-T1`, `P2-W3-T1`
  - **Subtasks:**
    - [x] P2-W3-T2.1: Enforce Tiered Guard: hard-lock mutating actions on playlists whose `FetchManifest.completeness` is `Truncated` or `Degraded`; render amber warning banner on `PartialHidden`.
    - [x] P2-W3-T2.2: After Sync POSTs complete, execute read-back `get_playlist_with_manifest` and emit `DispatchReport` comparing expected vs actual track counts while updating `playlist_cache` + `FetchManifest`.
    - [x] P2-W3-T2.3: In `StagedOp::UpgradeTracks`, match `added_items` before removing old UGC/OMV items, and when `preserve_position == true`, move each newly added `setVideoId` before the original track's successor before removing the old item.
  - **Verify:** `cargo test -p ytm-gui worker_exec`
  - **DoD:** Truncated/degraded fetches cannot be mutated, post-sync read-back verifies every playlist count, and ATV upgrades preserve playlist position safely.

- [x] **P2-W3-T3: Propagate `FetchManifest` Warnings, `MutationBatchReport` & Verified Upgrades to `ytm-cli`**
  - **Skills:** `surgical-patch`, `test-driven-development`
  - **Owns:** `crates/ytm-cli/src/mutations.rs`, `crates/ytm-cli/src/restore.rs`, `crates/ytm-cli/src/render.rs`
  - **Depends On:** `P2-W2-T1`
  - **Subtasks:**
    - [x] P2-W3-T3.1: Render `FetchManifest` summary (`declared vs fetched vs unavailable vs dropped`) in `inspect`, `dedupe`, `sort`, and `health`; block `--execute` on `Truncated`/`Degraded` unless `--force-incomplete` is passed.
    - [x] P2-W3-T3.2: In `handle_health`, verify `added_items` from `add_playlist_items` before removing upgraded UGC/OMV items and report full `MutationBatchReport` chunk stats.
  - **Verify:** `cargo test -p ytm-cli`
  - **DoD:** CLI enforces the same fetch-completeness guard and verified upgrade safety as GUI.

#### Phase 2 Exit Gate
```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- Update `PROGRESS.md` with Phase 2 gate evidence.

---

### Phase 3: Concurrency, 60 FPS Frame-Rate & Structural Architecture

#### Wave 3.1: Shared Services, Async Store & Normalized Enrichment (Parallel-Safe)

- [x] **P3-W1-T1: Extract Shared Pure Restore Diff & Post-Flight Diff Planners into `ytm-ops`**
  - **Skills:** `safe-refactor`, `test-driven-development`, `karpathy-guidelines`
  - **Owns:** `crates/ytm-ops/src/restore_plan.rs`, `crates/ytm-ops/src/lib.rs`, `crates/ytm-ops/tests/ops_algorithms_test.rs`
  - **Depends On:** `P2-W3-T3`
  - **Subtasks:**
    - [x] P3-W1-T1.1: Extract `plan_restore_diff(live: &Playlist, snapshot: &Playlist) -> RestoreDiffPlan` (`to_remove`, `to_add`, `allow_duplicates`, `target_video_order`) eliminating duplicated restore diff logic between `ytm-cli` and `ytm-gui`.
    - [x] P3-W1-T1.2: Add exhaustive unit tests covering duplicates, missing `set_video_id`, and identical snapshot no-op.
  - **Verify:** `cargo test -p ytm-ops restore_plan`
  - **DoD:** Single pure `plan_restore_diff` function is tested and ready for both CLI and GUI consumption.

- [x] **P3-W1-T2: Offload `SqliteStore` Calls via `tokio::task::spawn_blocking`**
  - **Skills:** `safe-refactor`, `surgical-patch`
  - **Owns:** `crates/ytm-store/src/db.rs`, `crates/ytm-store/src/snapshot.rs`
  - **Depends On:** `P2-W3-T1`
  - **Subtasks:**
    - [x] P3-W1-T2.1: Wrap `with_conn` inside async `EnrichmentCacheStore` and `SnapshotStore` trait methods using `tokio::task::spawn_blocking` so SQLite disk I/O never stalls Tokio worker threads.
    - [x] P3-W1-T2.2: Set `PRAGMA busy_timeout = 5000;` on connection open to eliminate transient `SQLITE_BUSY` errors under concurrent lanes.
  - **Verify:** `cargo test -p ytm-store`
  - **DoD:** Async SQLite trait calls run off Tokio async reactor threads with 5s busy timeout.

- [x] **P3-W1-T3: Normalize `ytm-enrich` Cache Keys & Add `artist_cache_first` Mode**
  - **Skills:** `safe-refactor`, `test-driven-development`
  - **Owns:** `crates/ytm-enrich/src/models.rs`, `crates/ytm-enrich/src/splitter.rs`, `crates/ytm-enrich/tests/enrich_test.rs`
  - **Depends On:** `P0-W2-T1`
  - **Subtasks:**
    - [x] P3-W1-T3.1: Normalize `build_cache_key` (NFKC, noise-tag strip, UGC `"Artist - Title"` split) so dirty YouTube titles hit existing cache and MusicBrainz cleanly.
    - [x] P3-W1-T3.2: Add `artist_cache_first: bool` (`false` default) to `EnrichmentConfig`; when enabled, short-circuit to fresh `artist_genres` cache before sleeping 1.1s for MusicBrainz.
    - [x] P3-W1-T3.3: Add test proving 5 tracks by the same cached artist incur 0 MusicBrainz HTTP calls when `artist_cache_first = true`.
  - **Verify:** `cargo test -p ytm-enrich`
  - **DoD:** Cache hit rate improves on noisy titles and `artist_cache_first` avoids redundant 1.1s MusicBrainz sleeps when enabled.

#### Wave 3.2: 3-Lane Async Worker, Memoized 60 FPS UI & Parallel Artwork Cache (Parallel-Safe)

- [x] **P3-W2-T1: Refactor GUI Worker to 3-Lane `tokio::sync::mpsc` with `CancellationToken` & Draft Planning**
  - **Skills:** `safe-refactor`, `lean-build`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/worker.rs`, `crates/ytm-gui/src/worker_plans.rs`, `crates/ytm-gui/src/worker_media.rs`, `crates/ytm-gui/src/navigation.rs`
  - **Depends On:** `P1-W3-T1`, `P3-W1-T2`, `P3-W1-T3`
  - **Subtasks:**
    - [x] P3-W2-T1.1: Replace blocking `std::sync::mpsc::recv()` loop in `worker.rs` with async `tokio::sync::mpsc::unbounded_receiver` dispatching across 3 lanes: `Interactive` (`tokio::spawn` parallel: Browse/Search/Lyrics/LoadPlaylist), `BackgroundJob` (`JobId` + `AtomicBool`/cancel token: IndexAll/Backup/Health/GenreSplit), and `MutationSerial` (serialized `Arc<tokio::sync::Mutex<()>>` for Sync).
    - [x] P3-W2-T1.2: Add "Cancel" button to status bar progress widget in `navigation.rs` wired to `WorkerCmd::CancelJob(job_id)`.
    - [x] P3-W2-T1.3: Allow `PlanDedupe`, `PlanSort`, `PlanSetOp`, and `ScanHealth` to plan against the local working draft `Playlist` snapshot (enabling chained `Dedupe → Health → Sort → Single Sync`) with an explicit "Plan vs Live YTM" option.
  - **Verify:** `cargo test -p ytm-gui worker`
  - **DoD:** Long-running indexing/enrichment never blocks interactive Browse/Search/Lyrics and cancels cleanly within 1 step.

- [x] **P3-W2-T2: Memoized `DraftRevision` Working-Copy Cache & Virtualized `repertoire_tree`**
  - **Skills:** `safe-refactor`, `karpathy-guidelines`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/draft_cache.rs`, `crates/ytm-gui/src/sidebar.rs`, `crates/ytm-gui/src/tab_router.rs`, `crates/ytm-gui/src/views/inspect_helpers.rs`, `crates/ytm-gui/src/views/split_diff.rs`, `crates/ytm-gui/src/views/repertoire_tree.rs`
  - **Depends On:** `P1-W1-T2`
  - **Subtasks:**
    - [x] P3-W2-T2.1: Create `DraftCache` keyed by `(playlist_id, DraftRevision)` storing `Arc<Playlist>`, sidebar track counts, cached LIS move count for `show_sort_stage_bar`, and cached `SplitDiffResult` (zero per-frame `Playlist` cloning or LIS recomputation).
    - [x] P3-W2-T2.2: Virtualize `views/repertoire_tree.rs` with flattened visible rows + `ScrollArea::show_rows` and remove the `O(N²)` inner selection-cloning loop (build drag payload lazily only when dragging starts).
    - [x] P3-W2-T2.3: Add unit test asserting `DraftCache::get` returns identical `Arc::ptr_eq` across repeated reads at the same `DraftRevision` and invalidates immediately on revision increment.
  - **Verify:** `cargo test -p ytm-gui draft_cache`
  - **DoD:** Steady-state frame rendering performs zero full-playlist clones, zero per-frame LIS solves, and `O(visible_rows)` work in `repertoire_tree`.

- [x] **P3-W2-T3: Split `views/inspect.rs` (<400 LOC), Fix Double-Spawned Actors & Event-Driven Repaints**
  - **Skills:** `safe-refactor`, `surgical-patch`
  - **Owns:** `crates/ytm-gui/src/views/inspect.rs`, `crates/ytm-gui/src/views/inspect_toolbar.rs`, `crates/ytm-gui/src/views/mod.rs`, `crates/ytm-gui/src/app.rs`, `crates/ytm-gui/src/player_ipc.rs`
  - **Depends On:** `P2-W3-T2`
  - **Subtasks:**
    - [x] P3-W2-T3.1: Extract header/toolbar/export bar from `views/inspect.rs` (currently 400 LOC) into `views/inspect_toolbar.rs` so both files remain `<250` lines.
    - [x] P3-W2-T3.2: Eliminate duplicate `PlayerController::new(None)` / `ArtworkCache::new(None)` construction in `app.rs::new` by passing `Option<egui::Context>` directly into `init_with_channels`.
    - [x] P3-W2-T3.3: Remove unconditional `ctx.request_repaint_after(500ms)` when idle (schedule repaint only while toast expiry is active or loading/playing); throttle `player_ipc.rs` `time-pos` repaints to `>= 200ms` intervals.
  - **Verify:** `wc -l crates/ytm-gui/src/views/inspect*.rs && cargo test -p ytm-gui player_ipc`
  - **DoD:** Every file under `views/inspect*.rs` is well under 400 LOC, zero orphan threads spawn at startup, and idle CPU repaint drops to 0 Hz.

- [x] **P3-W2-T4: Parallel Disk-Cached `ArtworkCache` with Failure Retry TTL**
  - **Skills:** `safe-refactor`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/artwork.rs`
  - **Depends On:** `P0-W2-T2`
  - **Subtasks:**
    - [x] P3-W2-T4.1: Replace serial `while let Ok(url)` loop with `tokio::sync::Semaphore` (6 concurrent fetches) and bounded SHA-256 disk cache under `ProjectDirs::cache_dir()/artwork/` (max 50 MiB LRU).
    - [x] P3-W2-T4.2: Replace permanent `failed: HashSet<String>` blacklist with `HashMap<String, Instant>` using a 60s retry TTL.
    - [x] P3-W2-T4.3: Add unit test for disk cache hit/eviction and failed-URL TTL expiry.
  - **Verify:** `cargo test -p ytm-gui artwork`
  - **DoD:** Browse/Playlist thumbnails load concurrently, persist across restarts on disk, and recover after transient network blips.

#### Wave 3.3: Cross-Platform `mpv` IPC, OS Keyring & CLI/Test Cleanup (Parallel-Safe)

- [x] **P3-W3-T1: Cross-Platform `mpv` IPC (`UnixStream` + Windows Named Pipe) & Keyring Fallback**
  - **Skills:** `safe-refactor`, `surgical-patch`
  - **Owns:** `crates/ytm-gui/src/player.rs`, `crates/ytm-gui/src/player_ipc.rs`, `crates/ytm-gui/src/auth_store.rs`, `crates/ytm-gui/Cargo.toml`
  - **Depends On:** `P3-W2-T3`
  - **Subtasks:**
    - [x] P3-W3-T1.1: Gate `std::os::unix::net::UnixStream` behind `#[cfg(unix)]` and add `#[cfg(windows)]` Named Pipe (`\\.\pipe\ytm-architect-mpv-<pid>`) transport + `mpv.exe` PATH resolution.
    - [x] P3-W3-T1.2: Gate `secret-tool` behind `#[cfg(unix)]` and add Windows credential storage fallback (via `auth.json` user-ACL notice or `keyring` crate) in `auth_store.rs`.
  - **Verify:** `cargo check -p ytm-gui --all-targets && cargo test -p ytm-gui player`
  - **DoD:** Zero unconditional Unix-only imports remain in `ytm-gui`; both Linux and Windows targets are structurally supported.

- [x] **P3-W3-T2: Adopt Shared `plan_restore_diff` in CLI & GUI + Split Oversized E2E Test File**
  - **Skills:** `safe-refactor`, `karpathy-guidelines`
  - **Owns:** `crates/ytm-cli/src/restore.rs`, `crates/ytm-gui/src/worker_exec.rs`, `crates/ytm-cli/tests/`
  - **Depends On:** `P3-W1-T1`, `P3-W2-T1`
  - **Subtasks:**
    - [x] P3-W3-T2.1: Replace duplicated snapshot restore diff math in `ytm-cli/src/restore.rs` and `ytm-gui/src/worker_exec.rs` with `ytm_ops::plan_restore_diff`.
    - [x] P3-W3-T2.2: Split `crates/ytm-cli/tests/e2e_pipeline_test.rs` (897 LOC) into focused test modules (`e2e_mock.rs`, `e2e_mutations_test.rs`, `e2e_tui_health_test.rs`) each `<400` LOC.
  - **Verify:** `cargo test -p ytm-cli && cargo test -p ytm-gui`
  - **DoD:** Restore diff logic exists in one place (`ytm-ops`) and every `.rs` file in the entire repo is `<= 400` lines.

#### Phase 3 Exit Gate
```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- Update `PROGRESS.md` with Phase 3 gate evidence.

---

### Phase 4: Daily-Driver YouTube Music Replacement

#### Wave 4.1: Full InnerTube Discovery, Search Tabs & Synced Lyrics (`ytm-core`) (Sequential)

- [x] **P4-W1-T1: Extend `ytm-core` with Filtered Search, Browse Continuations & Timed Lyrics**
  - **Skills:** `domain-modeling`, `test-driven-development`, `lean-build`
  - **Owns:** `crates/ytm-core/src/browse_models.rs`, `crates/ytm-core/src/parser/browse.rs`, `crates/ytm-core/src/client.rs`, `crates/ytm-core/src/traits.rs`, `crates/ytm-core/tests/browse_and_mutations_test.rs`
  - **Depends On:** `P2-W2-T1`
  - **Subtasks:**
    - [x] P4-W1-T1.1: Add `SearchFilter` (`Top`, `Songs`, `Videos`, `Albums`, `Artists`, `CommunityPlaylists`) + `SearchPage { tracks, cards, continuation }` and `BrowsePage.continuation: Option<String>` with continuation methods (`get_browse_continuation`, `search_catalog`).
    - [x] P4-W1-T1.2: Add `LyricsPayload` (`Plain(String)` | `Synced(Vec<TimedLyricLine>)`) parsing InnerTube `timedLyricsData` when present, falling back to plain lyrics.
    - [x] P4-W1-T1.3: Add fixture tests for filtered search, Browse continuation append, `FEmusic_explore` / `FEmusic_history` / `FEmusic_liked_albums`, and timed lyrics parsing.
  - **Verify:** `cargo test -p ytm-core browse_and_mutations`
  - **DoD:** `ytm-core` supports full YouTube Music search categories, paginated browse feeds, and synced lyrics parsing.

#### Wave 4.2: Gapless `mpv`, OS Media Controls & Persistent Queue (Parallel-Safe)

- [x] **P4-W2-T1: Gapless Prefetch Playback, Live Codec/Bitrate Telemetry & Actionable `yt-dlp` Errors**
  - **Skills:** `lean-build`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/player.rs`, `crates/ytm-gui/src/player_ipc.rs`
  - **Depends On:** `P3-W3-T1`
  - **Subtasks:**
    - [x] P4-W2-T1.1: Add `ActorCmd::PrefetchNext { video_id }` (resolving upcoming stream URL or issuing `loadfile ... append` ~15s before EOF) to eliminate 2–4s inter-track silence.
    - [x] P4-W2-T1.2: Observe `mpv` properties `audio-codec-name` and `audio-bitrate` into `PlayerSnapshot` for live quality verification (`Opus 160k` / `AAC 256k`).
    - [x] P4-W2-T1.3: Parse last `mpv`/`yt-dlp` error line from log/stderr into structured `PlaybackErrorKind` (`CookieExpired`, `GeoBlocked`, `AgeRestricted`, `MissingYtdlp`, `Network`) and support configurable skip-on-error.
  - **Verify:** `cargo test -p ytm-gui player`
  - **DoD:** Next track pre-resolves seamlessly, live codec/bitrate updates in snapshot, and stream failures explain exact root cause.

- [x] **P4-W2-T2: Integrate OS Media Controls (Linux MPRIS2 D-Bus + Windows SMTC)**
  - **Skills:** `lean-build`, `surgical-patch`
  - **Owns:** `crates/ytm-gui/src/media_controls.rs`, `crates/ytm-gui/Cargo.toml`, `Cargo.toml`
  - **Depends On:** `P3-W3-T1`
  - **Subtasks:**
    - [x] P4-W2-T2.1: Add `souvlaki` (cross-platform MPRIS2 + SMTC) behind graceful runtime initialization (never crash in headless/CI environments lacking D-Bus).
    - [x] P4-W2-T2.2: Sync track metadata (title, artist, album, duration, cover URL) and playback state to OS controls; route Play/Pause/Next/Prev/Stop/Seek events into `PlayerController` & `PlayQueue`.
  - **Verify:** `cargo check -p ytm-gui --all-targets && cargo test -p ytm-gui media_controls`
  - **DoD:** Hardware media keys, Hyprland/Waybar widgets, and Bluetooth controls govern playback cleanly with headless test safety.

- [x] **P4-W2-T3: Persistent `PlayQueue`, Startup `LM` Hydration & `video_id`-Correlated Lazy Lyrics**
  - **Skills:** `test-driven-development`, `migration`
  - **Owns:** `crates/ytm-store/src/playback_session.rs`, `crates/ytm-gui/src/play_queue.rs`, `crates/ytm-gui/src/playback_ops.rs`, `crates/ytm-gui/src/views/queue_lyrics.rs`
  - **Depends On:** `P4-W1-T1`, `P1-W2-T3`
  - **Subtasks:**
    - [x] P4-W2-T3.1: Persist and restore `PlayQueue` (tracks, `current_index`, volume, shuffle, repeat, last position, recently played) in SQLite `playback_session`.
    - [x] P4-W2-T3.2: Hydrate `play_queue.liked_video_ids` immediately from cached `LM` on startup so heart icons reflect true state before `LM` is clicked.
    - [x] P4-W2-T3.3: Fetch lyrics lazily (only when Lyrics panel is open or on demand) and correlate `LyricsLoaded { video_id, .. }` against `current_track()` so rapid skipping never overwrites lyrics with stale text; highlight active line for `LyricsPayload::Synced`; add "Save Queue as Playlist (Stage)".
  - **Verify:** `cargo test -p ytm-store playback_session && cargo test -p ytm-gui play_queue`
  - **DoD:** Queue and settings survive restarts, heart states match `LM` at launch, and lyrics never race or overwrite across track skips.

#### Wave 4.3: Global Search, Explore/History Views, Cookie Hygiene & Settings Panel (Parallel-Safe)

- [x] **P4-W3-T1: Build Global Search View, Explore/History Navigation & Universal `+ Stage Add`**
  - **Skills:** `lean-build`, `karpathy-guidelines`
  - **Owns:** `crates/ytm-gui/src/views/browse.rs`, `crates/ytm-gui/src/views/search.rs`, `crates/ytm-gui/src/views/mod.rs`
  - **Depends On:** `P4-W1-T1`, `P3-W2-T1`
  - **Subtasks:**
    - [x] P4-W3-T1.1: Create `views/search.rs` supporting all 6 `SearchFilter` tabs (`Top`, `Songs`, `Videos`, `Albums`, `Artists`, `Playlists`) with "Load More" continuation.
    - [x] P4-W3-T1.2: Add quick shortcuts in `views/browse.rs` for `Home`, `Explore (FEmusic_explore)`, `History (FEmusic_history)`, and `Library Albums (FEmusic_liked_albums)`, plus "Load More" continuation button.
    - [x] P4-W3-T1.3: Add `+ Stage Add to Playlist...` picker menu on Browse cards, Browse track rows, and Search results (strictly staging into `SyncQueue`).
  - **Verify:** `cargo check -p ytm-gui --all-targets`
  - **DoD:** Users can search any media type, browse Explore/History/Albums with pagination, and stage any discovered track into owned playlists.

- [x] **P4-W3-T2: Cookie Jar `0600` tmpfs Hygiene & Dedicated `Settings` Panel**
  - **Skills:** `surgical-patch`, `lean-build`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/auth_store.rs`, `crates/ytm-gui/src/worker_auth.rs`, `crates/ytm-gui/src/views/auth.rs`, `crates/ytm-gui/src/views/settings.rs`
  - **Depends On:** `P1-W2-T4`, `P3-W3-T1`
  - **Subtasks:**
    - [x] P4-W3-T2.1: Write `ytm-cookies.txt` and temporary browser cookie extracts inside `0600`-precreated files in `$XDG_RUNTIME_DIR` (tmpfs on Linux, user temp dir on Windows) and sweep stale files on startup/shutdown.
    - [x] P4-W3-T2.2: Build `views/settings.rs` exposing `config.toml` controls: `MutationPolicy` (`StrictLock` / `QuickLike`), `auto_refresh_browser_cookies`, `X-Goog-AuthUser` slot, Brand Account `X-Goog-PageId`, `hl`/`gl`, `mutation_delay_ms`, `mutation_chunk_size`, `max_pagination_pages`, `artist_cache_first`, and Last.fm/Deezer/ListenBrainz toggles.
    - [x] P4-W3-T2.3: Add unit tests for `0600` runtime cookie-path resolution and config roundtrip serialization.
  - **Verify:** `cargo test -p ytm-gui auth_store`
  - **DoD:** All power-user config & multi-account parameters are editable in GUI without hand-editing `config.toml`, and cookie jars live safely in `0600` runtime storage.

#### Phase 4 Exit Gate
```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
- Update `PROGRESS.md` with Phase 4 gate evidence.

---

### Phase 5: Information Architecture, UX Flow & Visual Polish

#### Wave 5.1: Custom `egui` Design System, Vector Icons & Shared Track/Badge Components (Parallel-Safe)

- [x] **P5-W1-T1: Custom `egui::Visuals` Theme & Embedded Vector Icon Font**
  - **Skills:** `hallmark`, `safe-refactor`
  - **Owns:** `crates/ytm-gui/src/theme.rs`, `crates/ytm-gui/src/icons.rs`, `crates/ytm-gui/src/main.rs`
  - **Depends On:** `P4-W3-T2`
  - **Subtasks:**
    - [x] P5-W1-T1.1: Define intentional dark surface hierarchy (`bg_canvas`, `bg_surface`, `bg_elevated`, crisp 1px borders, muted semantic green/amber/red, accessible contrast >= 4.5:1) in `theme.rs` — zero generic neon/AI-slop gradients.
    - [x] P5-W1-T1.2: Embed a crisp monochrome icon font or clean vector glyph constants in `icons.rs` replacing inconsistent emojis (`🏠`, `📻`, `🗑`, `✎`, `⋮⋮`, `::`) across the app.
  - **Verify:** `cargo check -p ytm-gui --all-targets`
  - **DoD:** Custom visuals and consistent icon tokens apply cleanly at app initialization without font fallback glitches.

- [x] **P5-W1-T2: Shared `TrackRow` & Clickable `FetchManifest` Health Badge Components**
  - **Skills:** `hallmark`, `karpathy-guidelines`
  - **Owns:** `crates/ytm-gui/src/views/components.rs`, `crates/ytm-gui/src/views/inspect_rows.rs`, `crates/ytm-gui/src/views/inspect_toolbar.rs`
  - **Depends On:** `P2-W3-T2`, `P3-W2-T3`
  - **Subtasks:**
    - [x] P5-W1-T2.1: Format all track durations consistently as `MM:SS` (`format_duration`) in `inspect_rows.rs` (replacing raw seconds `"245"` under `Dur(s)`).
    - [x] P5-W1-T2.2: Add clickable **Fetch Health Badge** in `inspect_toolbar.rs` (`247/247 Complete · 2m ago` green / `243/247 (4 hidden)` amber / `Truncated — Locked` red / `Cached 3d ago` gray) opening a `FetchManifest` detail popover (declared, fetched, available, unavailable, missing `setVideoId`, dropped renderers, pages).
  - **Verify:** `cargo check -p ytm-gui --all-targets`
  - **DoD:** Users see exact fetch completeness and cache freshness at a glance on every playlist with formatted `MM:SS` durations.

#### Wave 5.2: 5-Workspace Layout, Persistent Sync Staging Drawer & Command Palette (Sequential)

- [x] **P5-W2-T1: Consolidate 11 Flat Tabs into 5 Workspaces + Persistent Sync Staging Drawer**
  - **Skills:** `hallmark`, `brainstorming`, `safe-refactor`
  - **Owns:** `crates/ytm-gui/src/state.rs`, `crates/ytm-gui/src/tab_bar.rs`, `crates/ytm-gui/src/tab_router.rs`, `crates/ytm-gui/src/app.rs`
  - **Depends On:** `P5-W1-T1`, `P5-W1-T2`, `P4-W3-T1`
  - **Subtasks:**
    - [x] P5-W2-T1.1: Replace 11 flat `GuiTab` variants with 5 primary `Workspace` views (`HomeExplore` [Home/Search/Queue], `LibraryPlaylist` [Inspector/Split Diff/Repertoire Tree], `StudioTools` [Dedupe/Sort/SetOps/Health/GenreSplit], `SafetyHistory` [Snapshots/Backups], `SettingsAuth` [Auth/Settings]) reachable via `Ctrl+1..5`.
    - [x] P5-W2-T1.2: Add a collapsible right-hand **Sync Staging Drawer** (`Ctrl+Shift+S` or top-bar badge `Sync (N)`) visible from any workspace so users configure on the left and inspect staged changes live on the right before dispatching.
  - **Verify:** `cargo test -p ytm-gui tab_bar`
  - **DoD:** Navigation collapses from 11 cluttered tabs into 5 clean workspaces plus an always-accessible Sync Staging Drawer.

- [x] **P5-W2-T2: Explicit `FocusPane` Routing, Global Playback/Redo Keys & `Ctrl+K` Command Palette**
  - **Skills:** `hallmark`, `test-driven-development`
  - **Owns:** `crates/ytm-gui/src/interaction.rs`, `crates/ytm-gui/src/command_palette.rs`, `crates/ytm-gui/src/views/inspect_actions.rs`, `crates/ytm-gui/src/navigation.rs`
  - **Depends On:** `P5-W2-T1`
  - **Subtasks:**
    - [x] P5-W2-T2.1: Introduce `FocusPane` (`Sidebar`, `MainTable`, `Queue`, `Drawer`, `Modal`) so `ArrowUp`/`ArrowDown`/`Enter` never collide between sidebar and track table.
    - [x] P5-W2-T2.2: Wire global `Space` (Play/Pause when no text input is focused), `M` (Mute toggle), and `Ctrl+Shift+Z` (Redo stack for staged `SyncQueue` edits).
    - [x] P5-W2-T2.3: Implement `Ctrl+K` Command Palette with fuzzy search across playlists, cached tracks, workspaces, and Sync actions.
  - **Verify:** `cargo test -p ytm-gui interaction`
  - **DoD:** Zero keyboard focus collisions remain; `Space`, `M`, `Ctrl+Z`/`Ctrl+Shift+Z`, and `Ctrl+K` work predictably across all workspaces.

#### Wave 5.3: Review & Dispatch Cards, Snapshot Diff, Cherry-Pick & 3-Zone Player Bar (Parallel-Safe)

- [x] **P5-W3-T1: Expandable Review & Dispatch Cards & Visual Snapshot Diff Table**
  - **Skills:** `hallmark`, `karpathy-guidelines`
  - **Owns:** `crates/ytm-gui/src/views/sync.rs`, `crates/ytm-gui/src/views/genres_snapshots.rs`
  - **Depends On:** `P5-W2-T1`, `P3-W1-T1`
  - **Subtasks:**
    - [x] P5-W3-T1.1: Upgrade `views/sync.rs` (and Sync Drawer) to expandable cards showing exact track lists, `Before → After` order previews, target endpoint + chunk counts, optional raw JSON payload toggle, and live `Chunk X/Y · STATUS_SUCCEEDED` execution feed.
    - [x] P5-W3-T1.2: Upgrade `views/genres_snapshots.rs::show_snapshots` to compute and render `ytm_ops::plan_restore_diff` (`+Add`, `-Remove`, `⇄Reorder` preview table) before staging `RestoreSnapshot`.
  - **Verify:** `cargo check -p ytm-gui --all-targets`
  - **DoD:** Both Sync dispatch and Snapshot restore show complete track-level diffs before any POST is confirmed.

- [x] **P5-W3-T2: Cherry-Pick Checkboxes, Inline Playback & Custom Playlist Naming in Studio Tools**
  - **Skills:** `hallmark`, `surgical-patch`
  - **Owns:** `crates/ytm-gui/src/views/dedupe.rs`, `crates/ytm-gui/src/views/health.rs`, `crates/ytm-gui/src/views/set_ops.rs`
  - **Depends On:** `P5-W1-T2`, `P5-W2-T1`
  - **Subtasks:**
    - [x] P5-W3-T2.1: Add per-cluster checkboxes + `▶` preview play buttons in `views/dedupe.rs` and per-issue / per-ATV-upgrade checkboxes + `▶` comparison play buttons in `views/health.rs` (staging only checked items).
    - [x] P5-W3-T2.2: Add an editable target playlist title field in `views/set_ops.rs` (replacing hardcoded `"Set Diff (N)"`) plus `▶` preview play on result rows.
  - **Verify:** `cargo check -p ytm-gui --all-targets`
  - **DoD:** Users can audition tracks and selectively cherry-pick individual dedupe/health actions before staging.

- [x] **P5-W3-T3: Responsive 3-Zone Player Bar with Live Codec/Bitrate Badge & Quick Stage Add**
  - **Skills:** `hallmark`, `requesting-code-review`, `finishing-a-development-branch`
  - **Owns:** `crates/ytm-gui/src/views/player_bar.rs`
  - **Depends On:** `P4-W2-T1`, `P5-W1-T1`
  - **Subtasks:**
    - [x] P5-W3-T3.1: Refactor `views/player_bar.rs` into 3 non-overlapping responsive zones (Left: art + title/artist + heart + `+ Stage Add`; Center: transport + hover-timestamp seek bar; Right: live `mpv` codec/bitrate badge + mute + volume `0..100%` + Queue/Lyrics toggle).
    - [x] P5-W3-T3.2: Verify layout cleanly at minimum window width (`960x640`) without right-to-left overlap or clipped transport buttons.
  - **Verify:** `cargo test -p ytm-gui && cargo build --release --bin ytm-gui --bin ytm`
  - **DoD:** Player bar renders cleanly at `960x640` through ultrawide displays with live audio stream telemetry and zero clipping.

#### Phase 5 Exit Gate
```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --bin ytm-gui --bin ytm
```
- Verify every `.rs` source file is `<= 400` lines (`wc -l`).
- Update `PROGRESS.md` with Phase 5 & final release gate evidence.
