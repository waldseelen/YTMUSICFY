# SPEC.md

## 1. Domain Models (`ytm-core`)

### Track & Media Classification
- `Track`:
  - `video_id: String` (may be empty/sentinel for deleted/unavailable entries)
  - `set_video_id: Option<String>` (per-playlist-instance ID required for `ACTION_REMOVE_VIDEO` and `ACTION_MOVE_VIDEO_BEFORE`)
  - `position: usize` (0-based playlist index)
  - `title: String`
  - `artists: Vec<ArtistRef>` (`name: String`, `browse_id: Option<String>`)
  - `album: Option<AlbumRef>` (`name: String`, `browse_id: Option<String>`)
  - `duration_sec: Option<u32>`
  - `release_year: Option<u16>`
  - `music_video_type: MusicVideoType` (`AudioTrack` / `ATV`, `OfficialMusicVideo` / `OMV`, `UserGenerated` / `UGC`, `OfficialSourceMusic`, `Unknown`)
  - `availability: TrackAvailability` (`Available`, `Deleted`, `Private`, `RegionBlocked`, `Unplayable(String)`)

### InnerTube Authentication & Protocol Contracts
- **Credential Import:** Accepts raw DevTools HTTP headers, `Cookie:` string, or `ytmusicapi`-compatible `browser.json`. Stored in `<config_dir>/auth.json` (`0600` on Unix).
- **Dynamic Request Signing:** `SAPISIDHASH {unix_ts}_{sha1_hex("{unix_ts} {sapisid} https://music.youtube.com")}` using `__Secure-3PAPISID` (fallback `SAPISID`). Includes `Origin` / `X-Origin: https://music.youtube.com`, `X-Goog-AuthUser`, `X-Goog-PageId` (when configured), and `X-Goog-Visitor-Id`.
- **Context Pinning:** `context.client.clientName = "WEB_REMIX"`, `context.client.hl = "en"`, configurable `gl`, and `context.user.onBehalfOfUser` when Brand Account `x_goog_page_id` is present.
- **Logged-Out Canary Check:** `verify_session()` inspects `account/account_menu` (and/or authenticated `browse` `FEmusic_liked_playlists`) for explicit signed-in markers; HTTP `200 OK` without signed-in markers fails with `CoreError::AuthExpired`.
- **Pagination:** Exhausts both modern `continuationItemRenderer` (direct `continuationCommand.token` or nested `commandExecutorCommand.commands[].continuationCommand.token` -> `onResponseReceivedActions[0].appendContinuationItemsAction.continuationItems`) and legacy `continuations` tokens up to a configurable safety ceiling.
- **Liked Music (`LM` / `VLLM`):** Normalizes `"LM"` to `"VLLM"` for `browseId`.
- **Mutations (`POST /youtubei/v1/browse/edit_playlist`):**
  - Chunk size: max 50 actions per POST, 600ms inter-request delay, exponential backoff with jitter on `429`/`503`.
  - `ACTION_ADD_VIDEO`: `{ action, addedVideoId, dedupeOption? }`
  - `ACTION_REMOVE_VIDEO`: `{ action, setVideoId, removedVideoId }`
  - `ACTION_MOVE_VIDEO_BEFORE`: `{ action, setVideoId, movedSetVideoIdSuccessor? }` (omit `movedSetVideoIdSuccessor` to move item to end)

---

## 2. Persistence & Snapshots (`ytm-store`)

- Paths via `ProjectDirs::from("io", "ytm-architect", "ytm-architect")`:
  - `config_dir`: `auth.json`, `config.toml`, `genres.toml`
  - `data_dir`: `ytm.sqlite`, `snapshots/<playlist_id>/<timestamp>_<id>.json`
- SQLite tables:
  - `track_cache(video_id PRIMARY KEY, title, artists_json, album, duration_sec, release_year, music_video_type, updated_at)`
  - `artist_genres(artist_name_norm PRIMARY KEY, raw_tags_json, primary_genre, source, updated_at)`
  - `track_enrichment(cache_key PRIMARY KEY, video_id, tags_json, primary_genre, release_year, bpm, source, updated_at)`
  - `snapshots(id PRIMARY KEY, playlist_id, playlist_title, reason, created_at, item_count, payload_json)`
- Pre-mutation invariant: every mutating CLI/TUI action must write a snapshot (JSON + SQLite record) before invoking remote mutations.
- Export/import formats: `.json`, `.csv`, `.m3u8`.

---

## 3. Core Algorithms (`ytm-ops`)

### Deduplication
1. **Exact Pass:** Duplicate `video_id` entries within a playlist (each removable via unique `set_video_id`).
2. **Smart Fuzzy Pass:**
   - Normalize `title` and primary `artist`: NFKC, lowercase, strip parenthetical/bracketed noise (`(Official Video)`, `[Lyrics]`, `(Remastered YYYY)`, `(Live ...)`, `(HD/4K)`), extract `feat.`/`ft.`, normalize `&`/`and` and punctuation.
   - For `UGC` tracks lacking structured artist metadata or carrying `"Artist - Title"` format, extract artist prefix safely before matching.
   - `strsim::jaro_winkler` threshold default `0.90`.
   - **Duration Guard:** Reject fuzzy match when both durations exist and `|d1 - d2| > duration_tolerance_sec` (default `15s`) unless `ignore_duration == true`.
3. **Keeper Priority Scoring (Higher Wins):**
   - `Available` (`+1000`)
   - `AudioTrack` (`+100`) > `OfficialSourceMusic` (`+80`) > `OfficialMusicVideo` (`+50`) > `UserGenerated` (`+10`)
   - Clean non-live/non-remaster title (`+20`)
   - Tie-breaker: `HighestQuality` / `Oldest` (smaller `position`) / `Newest` (larger `position`).

### Multi-Key Sorting & LIS Minimal-Move Planner
- Keys: `artist`, `album`, `title`, `duration`, `year`, `genre`, `type`, `shuffle` (with artist-spread spacing). Each key supports `asc` / `desc`.
- Given current sequence $C$ and desired target sequence $T$ keyed by `set_video_id`:
  - Compute Longest Increasing Subsequence (LIS) of target positions along $C$.
  - Anchor LIS elements; emit ordered `MoveAction { set_video_id, moved_set_video_successor }` only for non-LIS elements such that applying moves sequentially to $C$ deterministically yields $T$.

### Set Algebra & Health Check
- Set ops: `diff`, `intersect`, `merge` with `ExactVideoId` or `FuzzyNormalized` matching.
- Health scan: classifies `Deleted`, `Private`, `RegionBlocked`, `Unplayable` entries, and optionally searches `ATV` equivalents for `UGC`/`OMV` items.

---

## 4. Enrichment & Genre Splitting (`ytm-enrich`)

- Lookup chain: SQLite cache (respecting TTL) -> MusicBrainz (`>= 1.1s` interval) -> optional fallback (Last.fm / Deezer / ListenBrainz) -> `genres.toml` hierarchical longest-pattern taxonomy match.
- `split-genre`: partitions tracks into `"{Playlist} - {Genre}"` buckets, grouping small buckets below `--min-tracks` into `"Other"` or skipping per configuration, idempotently avoiding duplicate additions.

---

## 5. CLI Surface (`ytm-cli`)

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
