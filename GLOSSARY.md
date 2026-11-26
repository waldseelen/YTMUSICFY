# GLOSSARY.md

Canonical domain terminology registry for `ytm-architect`. All crates, UI copy, logs, and specifications MUST use these locked terms and reject competing synonyms.

---

## 1. Canonical Domain Terms (Locked)

| Canonical Term | Crate / Boundary | Single-Line Definition |
| :--- | :--- | :--- |
| **`PlaylistAccess`** | `ytm-core` | Discriminated enum (`EditableOwned`, `LikedMusicAuto`, `ReadOnlyExternal`) encoding playlist mutability and ownership without parallel booleans. |
| **`FetchManifest`** | `ytm-core` | Per-fetch telemetry record capturing `declared_track_count`, `fetched_tracks`, availability counts, `missing_set_video_id`, `pages_fetched`, `completeness`, and `fetched_at`. |
| **`FetchCompleteness`** | `ytm-core` | Tiered completeness state (`Complete`, `PartialHidden`, `Truncated`, `Degraded`) governing UI badges and hard mutation locks. |
| **`MutationPolicy`** | `ytm-gui` | Governance mode (`StrictLock` default, `QuickLike` opt-in) controlling whether `SetLikeStatus` stages in `SyncQueue` or executes immediately with rollback. |
| **`StagedOp`** | `ytm-gui` / `ytm-store` | Closed enum of all remote-write operations (`AddTracks`, `RemoveTracks`, `MoveTracks`, `Reorder`, `CreatePlaylist`, `UpdatePlaylistMeta`, `DeletePlaylist`, `UpgradeTracks`, `RestoreSnapshot`, `SetLikeStatus`). |
| **`StagedEntry`** | `ytm-gui` / `ytm-store` | Sequenced item (`id`, `op: StagedOp`, `selected: bool`) inside the persistent local staging queue. |
| **`DraftRevision`** | `ytm-gui` | Monotonic `u64` counter incremented on every `SyncQueue` edit to key memoized working-copy drafts and invalidate stale UI caches. |
| **`PlanFingerprint`** | `ytm-gui` | Deterministic hash of `(playlist_id, DraftRevision, form_params)` locking tool `Stage` buttons against stale parameter previews. |
| **`DispatchPlan`** | `ytm-gui` | Pre-flight execution bundle derived from selected `StagedEntry` items detailing target playlists, endpoints, action payloads, and chunk counts before POST. |
| **`DispatchReport`** | `ytm-gui` | Post-sync verification summary aggregating per-entry `DispatchEntryOutcome` (`Succeeded`, `AbortedDrift`, `Failed`) and read-back `FetchManifest` results. |
| **`WorkerLane`** | `ytm-gui` | Async execution lane (`Interactive`, `BackgroundJob`, `MutationSerial`) isolating UI reads, cancellable batch jobs, and serialized remote mutations. |
| **`SessionState`** | `ytm-gui` | Explicit authentication finite-state machine (`Unauthenticated`, `Verifying`, `Authenticated`, `Expired`) replacing parallel boolean session flags. |

---

## 2. Forbidden Competing Synonyms

| Forbidden Term / Pattern | Canonical Replacement | Reason |
| :--- | :--- | :--- |
| `owned && !is_liked_music`, `is_editable`, ad-hoc `"LM"` string checks | `PlaylistAccess` | Eliminates illegal boolean combinations and scattered `"LM"`/`"VLLM"` special cases. |
| `FetchStatus`, `is_complete`, `partial_fetch: bool`, silent `truncated` logs | `FetchCompleteness` + `FetchManifest` | Preserves structured counts, dropped renderers, and tiered mutation locks. |
| `PendingMutation` (for queued ops), `QueuedOp`, `SyncItem`, `pending` vs `queued` | `StagedOp` + `StagedEntry` | Standardizes queued remote writes under one closed enum and entry wrapper. |
| `QuickLikeMode: bool`, `direct_post: bool`, `bypass_queue` | `MutationPolicy` | Keeps remote-write governance explicit with `StrictLock` as default. |
| `draft_version`, `cache_epoch`, `dirty: bool` | `DraftRevision` | Provides a single monotonic key for draft memoization and SQLite sync persistence. |
| `plan_hash`, `is_stale: bool`, `params_changed` | `PlanFingerprint` | Enforces exact parameter-to-preview binding before staging. |
| `SyncPreview`, `PreflightSummary`, `BatchPreview` | `DispatchPlan` | Unifies the pre-POST inspector contract in `Configure -> Preview -> Verified POST`. |
| `SyncResult`, `BatchOutcome`, `MutationSummary` (at dispatch layer) | `DispatchReport` (`MutationBatchReport` at `ytm-core` chunk layer) | Separates low-level chunk telemetry (`MutationBatchReport`) from verified queue-level outcomes (`DispatchReport`). |
| `TaskPool`, `WorkerMode`, `QueueKind` | `WorkerLane` | Locks the 3-lane async dispatch boundary (`Interactive`, `BackgroundJob`, `MutationSerial`). |
| `is_authenticated`, `is_verifying`, `auth_loading`, `logged_out: bool` | `SessionState` | Makes invalid concurrent auth states unrepresentable. |
