# SPEC.md

## 1. Domain Models & Protocol Contracts (`ytm-core`)

### Track, Playlist Access & Browse Models
- **`Track` (`models.rs`):**
  - `video_id: String` (may be empty/sentinel for deleted/unavailable entries)
  - `set_video_id: Option<String>` (per-playlist-instance ID required for `ACTION_REMOVE_VIDEO` and `ACTION_MOVE_VIDEO_BEFORE`)
  - `position: usize` (0-based playlist index)
  - `title: String`
  - `artists: Vec<ArtistRef>` (`name: String`, `browse_id: Option<String>`)
  - `album: Option<AlbumRef>` (`name: String`, `browse_id: Option<String>`)
  - `duration_sec: Option<u32>`
  - `release_year: Option<u16>`
  - `music_video_type: MusicVideoType` (`AudioTrack` / `ATV`, `OfficialMusicVideo` / `OMV`, `UserGenerated` / `UGC`, `OfficialSourceMusic` / `OSM`, `Unknown`)
  - `availability: TrackAvailability` (`Available`, `Deleted`, `Private`, `RegionBlocked`, `Unplayable(String)`)
  - `thumbnail_url: Option<String>` (`#[serde(default)]`, resolved via `effective_thumbnail_url()` with `https://i.ytimg.com/vi/{video_id}/mqdefault.jpg` fallback)
- **`PlaylistAccess` (`models.rs`):**
  - Closed enum replacing `owned && !is_liked_music` boolean combinations and ad-hoc `"LM"` checks:
    - `EditableOwned`: user-owned mutable playlist.
    - `LikedMusicAuto`: Liked Music (`"LM"` / `"VLLM"`); structural playlist mutations disabled (`SetLikeStatus` only).
    - `ReadOnlyExternal`: non-owned playlist; structural mutations locked.
- **`browse_models.rs`:**
  - `BrowsePageKind` (`Home`, `Artist`, `Album`, `Playlist`, `Unknown` via `from_browse_id`).
  - `BrowseCardKind` (`Track`, `Album`, `Artist`, `Playlist`) and `BrowseCard` (`kind`, `browse_id`, `video_id`, `title`, `subtitle`, `thumbnail_url`).
  - `BrowseSection` (`title`, `cards`, `tracks`) and `BrowsePage` (`browse_id`, `kind`, `title`, `subtitle`, `thumbnail_url`, `sections`, `tracks`).
  - `WatchNextResult` (`tracks`, `lyrics_browse_id`).

### `FetchManifest` & Tiered `FetchCompleteness` Guard
- **`FetchCompleteness`:**
  - `Complete`: fetched count matches header expectation with zero dropped renderers and complete pagination.
  - `PartialHidden { declared, fetched, hidden_estimate }`: YouTube Music hid/deleted items; renders amber warning badge and permits explicitly confirmed mutations.
  - `Truncated { pages_fetched, max_pages }`: pagination hit safety ceiling or aborted mid-continuation; **hard-locks** `Sort`, `Dedupe`, `Health`, and `Sync` mutations until cleanly re-fetched (CLI requires explicit `--force-incomplete`).
  - `Degraded { dropped_renderers }`: unrecognized/dropped track renderers encountered during tree parsing; **hard-locks** remote mutations until resolved.
- **`FetchManifest`:**
  - `declared_track_count: Option<usize>` (extracted from playlist header subtitle / collapsed count)
  - `fetched_tracks: usize`
  - `available_tracks: usize`
  - `unavailable_tracks: usize`
  - `missing_set_video_id: usize`
  - `pages_fetched: usize`
  - `completeness: FetchCompleteness`
  - `fetched_at: DateTime<Utc>`

### InnerTube Authentication & Wire Contracts
- **Credential Import:** Accepts raw DevTools HTTP headers, `Cookie:` string, or `ytmusicapi`-compatible `browser.json`. Stored in `<config_dir>/auth.json` (`0600` on Unix) and optionally OS Secret Service / credential store.
- **Dynamic Request Signing:** `SAPISIDHASH {unix_ts}_{sha1_hex("{unix_ts} {sapisid} https://music.youtube.com")}` using `__Secure-3PAPISID` (fallback `SAPISID`), refreshed per retry attempt. Includes `Origin` / `X-Origin: https://music.youtube.com`, `X-Goog-AuthUser`, `X-Goog-PageId` (when configured), and `X-Goog-Visitor-Id`.
- **Context Pinning:** `context.client.clientName = "WEB_REMIX"`, `context.client.hl = "en"`, configurable `gl`, and sibling `context.user.onBehalfOfUser` when Brand Account `x_goog_page_id` is present.
- **Transport & Retry Invariants:** HTTP/2 configured with `http2_max_header_list_size(64 * 1024)`. Retryable `INNERTUBE_API_KEY` discovery (`RwLock<Option<String>>`) recovers from transient startup offline errors. Read retries use jittered exponential backoff; non-idempotent POSTs use `RetryPolicy::MutationSafe` (no blind retries on ambiguous timeouts).
- **Logged-Out Canary Check:** `verify_session()` inspects `account/account_menu` (and/or authenticated `browse` `FEmusic_liked_playlists`) for explicit signed-in markers; HTTP `200 OK` without signed-in markers fails with `CoreError::AuthExpired`.
- **Pagination:** Exhausts both modern `continuationItemRenderer` (scanning all items so trailing `messageRenderer` notices do not truncate pagination; supporting `appendContinuationItemsAction` and `reloadContinuationItemsCommand`) and legacy `continuations` tokens, guarded by `seen_tokens` loop detection and `max_pagination_pages`.
- **Liked Music (`LM` / `VLLM`):** Normalizes `"LM"` to `"VLLM"` for `browseId`.
- **Mutations (`POST /youtubei/v1/browse/edit_playlist`):**
  - Chunk size: max 50 actions per POST, 600ms inter-request delay, jittered backoff on `429`/`503`.
  - `MutationAction::AddVideo` (`ACTION_ADD_VIDEO`): `{ action, addedVideoId, dedupeOption? }`
  - `MutationAction::RemoveVideo` (`ACTION_REMOVE_VIDEO`): `{ action, setVideoId, removedVideoId }`
  - `MutationAction::MoveVideoBefore` (`ACTION_MOVE_VIDEO_BEFORE`): `{ action, setVideoId, movedSetVideoIdSuccessor? }` (omit `movedSetVideoIdSuccessor` to move item to end)
  - `MutationAction::SetPlaylistName` (`ACTION_SET_PLAYLIST_NAME`): `{ action, playlistName }`
  - `MutationAction::SetPlaylistDescription` (`ACTION_SET_PLAYLIST_DESCRIPTION`): `{ action, playlistDescription }`
  - `MutationAction::SetPlaylistPrivacy` (`ACTION_SET_PLAYLIST_PRIVACY`): `{ action, playlistPrivacy }`
- **Additional Endpoints:**
  - `POST playlist/create`: creates playlist with chunked initial/appended `videoIds` (`409`/`400` post-create backoff on immediate `add_playlist_items`).
  - `POST playlist/delete`: deletes owned playlist after mandatory pre-delete snapshot.
  - `POST like/like` & `POST like/removelike`: toggles track like status with `{ target: { videoId } }`.
  - `POST browse`, `POST search`, `POST next`: library listing, home/explore/artist/album/lyrics browse, catalog search, and watch-next radio queues.
- **`MutationBatchReport` & `ChunkOutcome`:**
  - `send_edit_chunks` validates strict `STATUS_SUCCEEDED` on every `browse/edit_playlist` chunk (parsing `addToToastAction` failures into `CoreError::ApiError`).
  - `MutationBatchReport` preserves per-chunk `ChunkOutcome` telemetry (chunk index, action count, status, `added_items`, and partial-failure boundary) without dropping or overwriting earlier chunk results.

---

## 2. Persistence & Snapshots (`ytm-store`)

- Paths via `ProjectDirs::from("io", "ytm-architect", "ytm-architect")`:
  - `config_dir`: `auth.json`, `config.toml`, `genres.toml`
  - `data_dir`: `ytm.sqlite`, `snapshots/<playlist_id>/<timestamp>_<id>.json`
  - `cache_dir`: `artwork/` (bounded SHA-256 thumbnail cache)
- SQLite tables (WAL mode + `PRAGMA busy_timeout = 5000`, async calls dispatched via `tokio::task::spawn_blocking`):
  - `track_cache(video_id PRIMARY KEY, title, artists_json, album, duration_sec, release_year, music_video_type, updated_at)`
  - `artist_genres(artist_name_norm PRIMARY KEY, raw_tags_json, primary_genre, source, updated_at)`
  - `track_enrichment(cache_key PRIMARY KEY, video_id, tags_json, primary_genre, release_year, bpm, source, updated_at)`
  - `snapshots(id PRIMARY KEY, playlist_id, playlist_title, reason, created_at, item_count, payload_json)`
  - `playlist_cache(playlist_id PRIMARY KEY, title, payload_json, manifest_json, updated_at)` (`CachedPlaylistRecord { playlist, manifest, fetched_at }`, preserving legacy rows on schema upgrade and pruning deleted remote playlists)
  - `staged_ops(entry_id PRIMARY KEY, sort_order, op_json, selected, updated_at)` (`PersistedStagedOp` preserving queue order and selection across restarts)
  - `playback_session(id PRIMARY KEY, queue_json, current_index, volume, shuffle, repeat_mode, position_sec, updated_at)` (`PersistedPlaybackSession` for `PlayQueue` recovery)
  - `library_cache(id PRIMARY KEY, summaries_json, updated_at)`, `lyrics_cache(video_id PRIMARY KEY, browse_id, payload_json, updated_at)`, and `browse_cache(browse_id PRIMARY KEY, payload_json, updated_at)` (`schema v4` offline-first startup hydration)
  - `lyrics_meta(video_id PRIMARY KEY, offset_ms, pinned_provider, translation_lang, translated_json, updated_at)` (`schema v5` Phase 6 per-track offset, provider pin, and cached translations; backward-compatible `LyricsPayload::Rich` alongside `Plain`/`Synced`)
- Pre-mutation invariant: every mutating action must atomically persist a snapshot (JSON + SQLite `snapshots` record) before invoking remote mutations (`browse/edit_playlist`, `playlist/create`, `playlist/delete`); snapshot failure aborts the mutation.
- Export/import formats: `.json`, `.csv`, `.m3u8`.

---

## 3. Core Algorithms (`ytm-ops`)

### Deduplication
1. **Exact Pass:** Duplicate `video_id` entries within a playlist (each removable via unique `set_video_id`).
2. **Smart Fuzzy Pass:**
   - Normalize `title` and primary `artist`: NFKC, lowercase, strip parenthetical/bracketed noise (`(Official Video)`, `[Lyrics]`, `(Remastered YYYY)`, `(Live ...)`, `(HD/4K)`), extract `feat.`/`ft.`, normalize `&`/`and` and punctuation.
   - For `UGC` tracks lacking structured artist metadata or carrying `"Artist - Title"` format, extract artist prefix safely before matching.
   - `strsim::jaro_winkler` threshold default `0.90` (`0.4 * artist_sim + 0.6 * title_sim` with component floor `min(threshold, 0.82)`).
   - **Duration Guard:** Reject fuzzy match when both durations exist and `|d1 - d2| > duration_tolerance_sec` (default `15s`) unless `ignore_duration == true`.
3. **Keeper Priority Scoring (`keeper_score`, Higher Wins):**
   - Availability: `Available` (`+50`) vs unavailable (`-100`)
   - `MusicVideoType`: `AudioTrack` (`+40`) > `OfficialSourceMusic` (`+30`) > `OfficialMusicVideo` (`+15`) > `Unknown` (`0`) > `UserGenerated` (`-20`)
   - Clean non-live/non-remaster title (`+10`)
   - Metadata completeness: `album` present (`+5`), `artists` non-empty (`+3`), `release_year` present (`+2`)
   - Tie-breaker (`KeeperStrategy`): `Quality` (highest `keeper_score`, ties broken by smallest `position`), `Oldest` (available preferred, then smallest `position`), `Newest` (available preferred, then largest `position`).

### Multi-Key Sorting & LIS Minimal-Move Planner
- Keys: `artist`, `album`, `title`, `duration`, `year`, `genre`, `type`, `shuffle` (with artist-spread spacing). Each key supports `asc` / `desc`.
- Given current sequence $C$ and desired target sequence $T$ keyed by `set_video_id`:
  - Compute Longest Increasing Subsequence (LIS) of target positions along $C$.
  - Anchor LIS elements; emit ordered `MoveAction { set_video_id, moved_set_video_successor }` only for non-LIS elements such that applying moves sequentially to $C$ deterministically yields $T$.

### Set Algebra, Health Check & Restore Diff
- Set ops: `diff`, `intersect`, `merge` with `ExactVideoId` or `FuzzyNormalized` matching.
- Health scan: classifies `Deleted`, `Private`, `RegionBlocked`, `Unplayable` entries, and optionally searches `ATV` equivalents for `UGC`/`OMV` items.
- Restore diff (`plan_restore_diff`): pure planner shared by `ytm-cli` and `ytm-gui` computing `to_remove`, `to_add`, `allow_duplicates`, and `target_video_order` between live `Playlist` and `Snapshot`.

---

## 4. Enrichment & Genre Splitting (`ytm-enrich`)

- Lookup chain: SQLite cache (respecting TTL; optional `artist_cache_first = true` fast path short-circuits to fresh `artist_genres` before MusicBrainz sleep) -> MusicBrainz (`>= 1.1s` interval) -> optional fallback (Last.fm / Deezer / ListenBrainz) -> `genres.toml` hierarchical longest-pattern taxonomy match.
- `split-genre`: partitions tracks into `"{Playlist} - {Genre}"` buckets, grouping small buckets below `--min-tracks` into `"Other"` or skipping per configuration, idempotently avoiding duplicate additions.

---

## 5. Desktop GUI Pipeline (`ytm-gui`)

### `Configure -> Preview -> Verified POST` Contract
1. **Configure & Plan (`PlanFingerprint`):** Tool views compute plans against the selected `Playlist` (live or local working draft) and bind the preview to a `PlanFingerprint` of `(playlist_id, DraftRevision, form_params)`. Parameter or selection changes invalidate staging until re-scanned. Mutations on `PlaylistAccess::LikedMusicAuto`, `PlaylistAccess::ReadOnlyExternal`, or `FetchCompleteness::{Truncated, Degraded}` playlists are hard-locked.
2. **Stage (`SyncQueue` + `StagedOp`):** All remote writes enter the persistent SQLite-backed `SyncQueue` as `StagedEntry { id, op, selected }` across the closed `StagedOp` set (`AddTracks`, `RemoveTracks`, `MoveTracks`, `Reorder`, `CreatePlaylist`, `UpdatePlaylistMeta`, `DeletePlaylist`, `UpgradeTracks`, `RestoreSnapshot`, `SetLikeStatus`), bumping `DraftRevision`. `MutationPolicy::StrictLock` (default) stages `SetLikeStatus`; opt-in `MutationPolicy::QuickLike` dispatches immediate like/unlike with failure rollback and `LM` cache sync.
3. **Preview (`DispatchPlan`):** The Sync inspector renders every selected `StagedEntry` with exact target playlist, track IDs, endpoint (`browse/edit_playlist`, `playlist/create`, `playlist/delete`, `like/*`), and chunk count; modal confirmation requires explicit button click (and typed playlist title match for `DeletePlaylist`).
4. **Verified POST (`DispatchReport`):** Executed sequentially on `WorkerLane::MutationSerial`:
   - Mandatory atomic pre-flight SQLite + JSON snapshot per affected playlist (aborts before any POST on snapshot error).
   - Live drift verification before `Reorder` (recomputing LIS against live `setVideoId`s or aborting entry on `AbortedDrift`).
   - Verified multi-step sequencing for `MoveTracks` (`AddTracks` -> non-empty `added_items` check -> `RemoveTracks`) and `UpgradeTracks` (position-preserving move before successor when `preserve_position == true`).
   - Per-entry `DispatchEntryOutcome` (`Succeeded`, `AbortedDrift`, `Failed`) and selective `SyncQueue` pruning so completed entries are never re-executed after a partial failure.
   - Post-mutation read-back `get_playlist_with_manifest` updating `playlist_cache` and emitting `DispatchReport` count verification.

### Session Governance & Dalga UI Contracts
- `SessionState` (`Unauthenticated`, `Verifying`, `Authenticated(SessionStatus)`, `Expired { reason, last_known }`) tracks authentication without parallel loading booleans.
- `auto_refresh_browser_cookies` defaults to `false`; startup never executes silent `yt-dlp --cookies-from-browser` extraction without explicit opt-in or user action.
- `GuiAppConfig` persists `theme_mode: ThemeMode` (`System | Dark | Light`), `accent_palette: AccentPalette` (`Emerald | Gold | YtmPink`), `dynamic_accent: bool` (default `true`), `ui_lang: UiLang` (`System | En | Tr`), `greetings_enabled: bool` (default `true`), `ticker_speed: f32` (default `48.0`), `reduced_motion: bool`, `preferred_external_browser: Option<String>`, `recent_searches: Vec<String>`, `tray_enabled: bool` (default `true`), `close_to_tray: bool` (default `true`), `mini_player_opacity_on_idle: bool` (default `false`), and `mini_player_layouts: HashMap<String, MiniPlayerGeometry>` with `#[serde(default)]` backward compatibility for `v0.1.x` / `v0.2.x` `config.toml`.
- `StatusMessage` and `StatusPriority` (`ErrorWarning > ActionResult > ContextualHint > Greeting`) drive the Status Ribbon message bus and 50-entry history ring.
- `TrayBackend` (`LinuxKsniTray`, `WindowsTray`, `NoopTrayBackend`), `CloseAction` (`None | HideToTray | Quit`), `MiniPlayerMode` (`Compact | Expanded`), `ContextTarget` (`Track | Album | Artist | Playlist`), and `SHORTCUTS` drive desktop lifecycle, secondary Mini Player viewport, unified right-click menus, and keyboard navigation.

---

## 6. CLI Surface (`ytm-cli`)

| Command | Mutates Remote? | Dry-Run Default? |
| :--- | :--- | :--- |
| `ytm auth import [--file <path>]` / `ytm auth status` | No | N/A |
| `ytm list [--json]` / `ytm inspect <playlist\|LM> [--json]` | No | N/A |
| `ytm backup <playlist\|LM\|all> [--format json\|csv\|m3u8] [--out <dir>]` | No | N/A |
| `ytm snapshots list\|show\|restore <snapshot_id> [--execute]` | `restore` only | Yes (`--execute` required) |
| `ytm dedupe <playlist> [--threshold 0.90] [--duration-tol 15] [--keep quality\|oldest\|newest] [--execute]` | Yes | Yes (`--execute` required) |
| `ytm sort <playlist> --by <keys> [--execute]` | Yes | Yes (`--execute` required) |
| `ytm set <diff\|intersect\|merge> <a> <b> [--fuzzy] [--into <target>] [--execute]` | Only with `--into` | Yes (`--execute` required) |
| `ytm health <playlist\|all> [--find-official] [--prune-unavailable] [--execute]` | Only with `--prune-unavailable` / upgrade | Yes (`--execute` required) |
| `ytm split-genre <playlist\|LM> [--config <genres.toml>] [--min-tracks 5] [--execute]` | Yes | Yes (`--execute` required) |
| `ytm tui` | Interactive confirm required | Yes |

- Mutating CLI subcommands display `FetchManifest` completeness telemetry, block `--execute` on `Truncated`/`Degraded` fetches unless `--force-incomplete` is passed, and surface `MutationBatchReport` chunk summaries after execution.
