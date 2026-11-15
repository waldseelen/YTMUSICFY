# ytm-architect

Local-first, cross-platform (**Windows & Linux**) YouTube Music library & playlist engineering suite written in pure Rust.

## Features

- **Resilient InnerTube Auth:** `SAPISIDHASH` signing, multi-account (`X-Goog-AuthUser`), Brand Account (`X-Goog-PageId` + `onBehalfOfUser`), and logged-out `200 OK` canary verification.
- **Safe-by-Default Mutations:** Every mutating command defaults to `--dry-run` preview tables and persists an atomic local snapshot (JSON + SQLite) before sending any write request when `--execute` is passed.
- **Exact & Fuzzy Deduplication:** Exact `videoId` removal plus Unicode-normalized Jaro-Winkler matching (`90%` default) with `±15s` duration guard and quality-tier keeper selection (`ATV > OSM > OMV > UGC`).
- **Minimal-Move Sorting (LIS):** Composable sort keys (`artist`, `album`, `title`, `duration`, `year`, `genre`, `type`, `shuffle`) paired with an `O(N log N)` Longest Increasing Subsequence move planner to minimize `ACTION_MOVE_VIDEO_BEFORE` API calls.
- **Set Algebra, Health Scanner & Genre Splitting:** `diff` / `intersect` / `merge`, deleted/private/unplayable scanning + `UGC -> ATV` upgrade search, and SQLite-cached MusicBrainz + Deezer/Last.fm/ListenBrainz genre splitting.
- **Interactive TUI & CLI:** `clap` CLI + 5-tab `ratatui` terminal dashboard (`Playlists`, `Dedupe`, `Sort`, `Health`, `Genres`).

## Build & Install

Requires Rust `1.88+` (Edition 2024). No Python or Node.js runtime dependencies are needed.

### Linux / macOS

```bash
cargo build --release
./target/release/ytm --help
```

### Windows (PowerShell)

```powershell
cargo build --release
.\target\release\ytm.exe --help
```

## Configuration & Storage Paths

Files are stored in standard OS locations via `ProjectDirs("io", "ytm-architect", "ytm-architect")`:

- **Linux:** `~/.config/ytm-architect/{auth.json, config.toml, genres.toml}` and `~/.local/share/ytm-architect/{ytm.sqlite, snapshots/}`
- **Windows:** `%APPDATA%\ytm-architect\ytm-architect\config\` and `%LOCALAPPDATA%\ytm-architect\ytm-architect\data\`

Copy starter configs from [`config/config.example.toml`](config/config.example.toml) and [`config/genres.example.toml`](config/genres.example.toml). Set `YTM_LASTFM_API_KEY` optionally to enable Last.fm tag lookups.

## Browser Authentication Setup

1. Open `https://music.youtube.com` in Chromium/Firefox while signed in.
2. Open DevTools (`F12`) -> **Network** -> filter `/youtubei/v1/browse` -> right-click an authenticated POST request -> **Copy Request Headers** (or save `browser.json` / raw `Cookie:` header).
3. Save to a temporary text file (or pipe via stdin) and import:

### Linux

```bash
ytm auth import --file headers.txt
ytm auth status
```

### Windows (PowerShell)

```powershell
Get-Content headers.txt -Raw | .\target\release\ytm.exe auth import
.\target\release\ytm.exe auth status
```

**Brand Accounts & Secondary Channels:** Ensure your copied headers include `X-Goog-AuthUser` (e.g. `1`) and `X-Goog-PageId` (for Brand Accounts). `ytm-architect` automatically forwards `onBehalfOfUser` inside InnerTube `context.user` alongside sibling auth headers and restricts `auth.json` to `0600` permissions on Unix.

## CLI Command Reference

All mutating commands run safely in preview mode unless `--execute` is specified.

```bash
# Library inspection & local snapshots
ytm list
ytm inspect LM
ytm backup all --format csv --out ./backups
ytm snapshots list
ytm snapshots show <SNAPSHOT_ID>
ytm snapshots restore <SNAPSHOT_ID> --execute

# Deduplication (dry-run first, then execute)
ytm dedupe <PLAYLIST_ID> --threshold 0.90 --duration-tol 15 --keep quality
ytm dedupe <PLAYLIST_ID> --execute

# Minimal-move sorting & artist-spread shuffle
ytm sort <PLAYLIST_ID> --by artist:asc,year:asc,title:asc
ytm sort <PLAYLIST_ID> --by shuffle --seed 42 --execute

# Set algebra (diff, intersect, merge)
ytm set diff LM <PLAYLIST_ID> --fuzzy --into <TARGET_ID> --execute

# Health audit & UGC/OMV -> official ATV upgrade
ytm health <PLAYLIST_ID> --find-official --prune-unavailable --upgrade --execute

# Genre enrichment & idempotent split into "<Playlist> - <Genre>"
ytm split-genre <PLAYLIST_ID> --min-tracks 5 --execute

# Interactive 5-tab dashboard
ytm tui
```

## Workspace Architecture

- `crates/ytm-core` — InnerTube client (`InnerTubeClientTrait`), `SAPISIDHASH` signing, recursive JSON parser, pagination, chunked mutations
- `crates/ytm-store` — Cross-platform paths, atomic writes, SQLite cache & snapshots, JSON/CSV/M3U8 exports
- `crates/ytm-ops` — Dedupe, `O(N log N)` LIS minimal-move sort, set algebra, health scanner
- `crates/ytm-enrich` — Rate-limited MusicBrainz + fallback enrichment & longest-match `genres.toml` taxonomy
- `crates/ytm-cli` — `ytm` CLI binary and Ratatui TUI

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT` and `LICENSE-APACHE`.
