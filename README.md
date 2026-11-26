# ytm-architect

Local-first, pure-Rust **YouTube Music desktop studio (`ytm-gui`) + CLI/TUI (`ytm`)** for **Linux & Windows**.

Instead of wrapping YouTube Music in Electron or a heavy WebView, `ytm-architect` speaks YouTube Music's own **InnerTube HTTP API** directly using your local browser session (`SAPISIDHASH` + `Cookie`), backs audio playback with an ultra-light **headless `mpv` IPC player**, and enforces a strict **`Configure -> Preview -> Verified POST`** mutation lock so nothing alters your remote library without explicit inspection and snapshot protection.

---

## Why `ytm-architect`?

- **Zero Electron / WebView Bloat:** Native `eframe`/`egui` desktop studio + `ratatui` TUI + `clap` CLI compiled to native binaries.
- **No Cloud Relay, No OAuth Headache:** Uses your existing signed-in Chromium/Brave/Firefox/Edge session locally (plus optional Brand Account `X-Goog-PageId` & multi-account `X-Goog-AuthUser` support).
- **Strict Mutation Governance (`Configure -> Preview -> Verified POST`):**
  - Every playlist edit (`AddTracks`, `RemoveTracks`, `MoveTracks`, `Reorder`, `UpdatePlaylistMeta`, `CreatePlaylist`, `DeletePlaylist`, `UpgradeTracks`, `RestoreSnapshot`) stages locally in SQLite-persisted `SyncQueue` before sending any POST request.
  - `FetchManifest` proofs compare YouTube Music's declared playlist track count against actually fetched/dropped items (`Complete`, `PartialHidden`, `Truncated`, `Degraded`) and lock unsafe writes on truncated reads.
  - Every plan is bound to a `PlanFingerprint` `(playlist_id, draft_revision, form_hash)` to block stale dispatches, and every remote batch auto-saves a SQLite pre-mutation snapshot (`Snapshots`) before execution.
  - `♥` Like actions default to `StrictLock` (staged in Sync) with an optional `QuickLike` setting in **Settings & Auth** (optimistic rollback + `LM` cache sync).
- **Daily-Driver Music Player:** Gapless-ready `mpv` JSON IPC (Unix domain sockets on Linux, Named Pipes on Windows), 15s pre-EOF track prefetch, persistent `PlayQueue`, `Radio` seeding, synced/plain `Lyrics`, MPRIS2 (Linux) & SMTC (Windows) media keys (`souvlaki`), live codec/bitrate telemetry, and `Ctrl+K` Command Palette.

---

## Desktop Studio (`ytm-gui`) Workspaces

| Workspace | Shortcut | What It Includes |
| :--- | :--- | :--- |
| **Home & Explore** | `Ctrl+1` | Home shelves, Explore, History, Library Albums, Full Catalog Search (Songs/Videos/Albums/Artists/Playlists), Play Queue & Synced Lyrics |
| **Library & Playlist** | `Ctrl+2` | Virtualized Repertoire Search, Track Inspector (`MM:SS`), inline Playback, Split-Diff comparison, Metadata editor, Drag & Drop staging |
| **Studio Tools** | `Ctrl+3` | Exact/Fuzzy **Dedupe**, `O(N log N)` LIS minimal-move **Sort**, **Set Algebra** (`A \ B`, `A ∩ B`, `A ∪ B`), **Health Scanner** (`UGC -> Official ATV` upgrade), **Genre Split** |
| **Safety & History** | `Ctrl+4` | SQLite pre-mutation **Snapshots** with visual line-by-line restore diff (`plan_restore_diff`), full-page **Sync** review & dispatch |
| **Settings & Auth** | `Ctrl+5` | `config.toml` Governance (`StrictLock` / `QuickLike`, opt-in browser cookie refresh, InnerTube pacing, `artist_cache_first`), Cookie Import, Stream Quality |

- **Global Shortcuts:** `Ctrl+K` Command Palette · `Ctrl+Shift+S` Toggle Sync Staging Drawer · `Ctrl+Z` / `Ctrl+Shift+Z` Undo/Redo staged op · `Ctrl+Enter` Execute Sync · `Space` Play/Pause · `M` Mute · `Ctrl+F` Focus search · `Alt+Left/Right` Back/Forward history.

---

## Download Pre-Built Releases (Linux & Windows)

Pre-built binaries and verified checksums are published on [**GitHub Releases**](https://github.com/waldseelen/YTMUSICFY/releases).

| Platform | Target | Archive | Binaries Included |
| :--- | :--- | :--- | :--- |
| **Linux (x86_64)** | `x86_64-unknown-linux-gnu` | `ytm-architect-v0.1.0-linux-x86_64.tar.gz` | `ytm-gui`, `ytm` |
| **Windows (x86_64)** | `x86_64-pc-windows-msvc` | `ytm-architect-v0.1.0-windows-x86_64.zip` | `ytm-gui.exe`, `ytm.exe` |

### Verify Downloads (`SHA256SUMS.txt` — Release `v0.1.0`)

| Artifact | SHA-256 |
| :--- | :--- |
| `ytm-architect-v0.1.0-linux-x86_64.tar.gz` | `eaa6c5c1a59131d7496b2119420a167ba0f7e468081e49bb56f5b801548005c6` |
| `ytm` (Linux x86_64 CLI/TUI) | `808e48ce74c4fff8cfcb1e4c319afcc85e4e0c5a632c18c33b4d870a03e9f062` |
| `ytm-gui` (Linux x86_64 GUI) | `57c7b1d27fc2a5e237cb89e05057b17974e670499aa0f6fc8cae4b02134f4fa7` |
| `ytm-architect-v0.1.0-windows-x86_64.zip` | `189606203cab13101962b4a88dcb9e4cb5a75101cb90e5bb9b5922f470273a71` |
| `ytm.exe` (Windows x86_64 CLI/TUI) | `f4d226542dc286a7fa15ac93352e0d2bee52870a5f75b4ec0053e149d6b332a7` |
| `ytm-gui.exe` (Windows x86_64 GUI) | `b7f484e5319e81d91f589c661c666ae98982ab840043eef0e758bc8521403d9f` |

#### Linux
```bash
tar -xzf ytm-architect-v0.1.0-linux-x86_64.tar.gz
sha256sum -c SHA256SUMS.txt --ignore-missing
./ytm-architect-v0.1.0-linux-x86_64/ytm-gui
```

#### Windows (PowerShell)
```powershell
Expand-Archive .\ytm-architect-v0.1.0-windows-x86_64.zip -DestinationPath .\ytm-architect
Get-FileHash .\ytm-architect\ytm-architect-v0.1.0-windows-x86_64\ytm-gui.exe -Algorithm SHA256
.\ytm-architect\ytm-architect-v0.1.0-windows-x86_64\ytm-gui.exe
```

> **Audio Playback Prerequisites:** Install [`mpv`](https://mpv.io/) and [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) on `PATH` for audio streaming in `ytm-gui`. All library inspection, staging, dedupe, sorting, set algebra, export, and sync features work standalone without external binaries.

---

## Build From Source

Requires **Rust 1.85+** (Edition 2024).

### Linux
```bash
# Arch / Omarchy / Debian / Ubuntu build deps (dbus + xkbcommon + gtk3 + openssl)
cargo build --release --locked --bin ytm-gui --bin ytm
./target/release/ytm-gui
```

### Windows (PowerShell)
```powershell
cargo build --release --locked --bin ytm-gui --bin ytm
.\target\release\ytm-gui.exe
```

---

## Authentication Setup

1. Sign in to `https://music.youtube.com` in Chrome, Brave, Edge, or Firefox.
2. Either:
   - Use **Settings & Auth -> Auth -> Import from Browser (`yt-dlp`)** explicitly inside `ytm-gui` (opt-in; silent startup cookie scraping is disabled by default), **OR**
   - Open DevTools (`F12`) -> **Network** -> filter `/youtubei/v1/browse` -> right-click an authenticated POST request -> **Copy Request Headers**, then import via GUI paste/file or CLI:

```bash
# Linux
ytm auth import --file headers.txt
ytm auth status
```

```powershell
# Windows (PowerShell)
Get-Content headers.txt -Raw | .\ytm.exe auth import
.\ytm.exe auth status
```

**Storage Security:** On Linux, credentials save to OS Secret Service (`secret-tool`) when available plus a `0600` fallback in `auth.json`, and temporary `mpv` cookie jars live in `$XDG_RUNTIME_DIR/ytm-architect/` (`0700`/`0600` tmpfs).

---

## Configuration & Storage Paths

Standard OS paths via `ProjectDirs("io", "ytm-architect", "ytm-architect")`:

- **Linux:**
  - Config: `~/.config/ytm-architect/{auth.json, config.toml, genres.toml}`
  - Data / Cache / Snapshots: `~/.local/share/ytm-architect/{ytm.sqlite, snapshots/}`
- **Windows:**
  - Config: `%APPDATA%\ytm-architect\ytm-architect\config\`
  - Data / Cache / Snapshots: `%LOCALAPPDATA%\ytm-architect\ytm-architect\data\`

Starter templates live in [`config/config.example.toml`](config/config.example.toml) and [`config/genres.example.toml`](config/genres.example.toml). Optionally set `YTM_LASTFM_API_KEY` for Last.fm genre tags.

---

## CLI Command Reference (`ytm`)

Mutating CLI commands run in **dry-run preview** mode unless `--execute` is passed, and automatically save a local snapshot before touching remote playlists.

```bash
# Inspect library & export backups
ytm list
ytm inspect LM
ytm backup all --format csv --out ./backups
ytm snapshots list
ytm snapshots show <SNAPSHOT_ID>
ytm snapshots restore <SNAPSHOT_ID> --execute

# Dedupe (preview first, then execute)
ytm dedupe <PLAYLIST_ID> --threshold 0.90 --duration-tol 15 --keep quality
ytm dedupe <PLAYLIST_ID> --execute

# O(N log N) LIS minimal-move sort & seeded artist-spread shuffle
ytm sort <PLAYLIST_ID> --by artist:asc,year:asc,title:asc
ytm sort <PLAYLIST_ID> --by shuffle --seed 42 --execute

# Set algebra (diff, intersect, merge)
ytm set diff LM <PLAYLIST_ID> --fuzzy --into <TARGET_ID> --execute

# Health scan & UGC -> Official ATV upgrade
ytm health <PLAYLIST_ID> --find-official --prune-unavailable --upgrade --execute

# Genre enrichment & idempotent split
ytm split-genre <PLAYLIST_ID> --min-tracks 5 --execute

# Terminal 5-tab dashboard
ytm tui
```

---

## Workspace Architecture (6 Crates)

```
crates/
├── ytm-core    # InnerTube HTTP client, SAPISIDHASH signing, FetchManifest, Search/Browse/Lyrics parsers, chunked POST verification
├── ytm-store   # SQLite cache (manifest_json, staged_ops, playback_session), snapshots, JSON/CSV/M3U8 exports, OS paths
├── ytm-ops     # Pure playlist engines: Dedupe, O(N log N) LIS Sort, Set Algebra, Health, Restore Diff
├── ytm-enrich  # Normalized SQLite enrichment cache + MusicBrainz / Deezer / Last.fm / ListenBrainz genre pipeline
├── ytm-cli     # `ytm` CLI binary + 5-tab Ratatui TUI
└── ytm-gui     # `ytm-gui` 5-Workspace eframe/egui Studio, 3-lane Tokio worker, mpv IPC player, MPRIS2/SMTC bridge
```

For architectural contracts and domain terms, see [`ARCHITECTURE.md`](ARCHITECTURE.md), [`SPEC.md`](SPEC.md), and [`GLOSSARY.md`](GLOSSARY.md).

---

## License

Dual-licensed under `MIT OR Apache-2.0`. See [`LICENSE-MIT`](LICENSE-MIT) and [`LICENSE-APACHE`](LICENSE-APACHE).
