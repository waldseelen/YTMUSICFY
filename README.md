# ytm-architect

Local-first, cross-platform (**Windows & Linux**) YouTube Music library & playlist engineering suite written in pure Rust.

## Features

- **Resilient InnerTube Auth:** `SAPISIDHASH` signing, multi-account (`X-Goog-AuthUser`), Brand Account (`X-Goog-PageId` + `onBehalfOfUser`), and logged-out `200 OK` canary verification.
- **Safe-by-Default Mutations:** Mutating commands default to `--dry-run` preview tables and write an atomic local snapshot (JSON + SQLite) before sending any write request when `--execute` is passed.
- **Exact & Fuzzy Deduplication:** Exact `videoId` removal plus Unicode-normalized Jaro-Winkler matching with duration guard and quality-tier keeper selection (`ATV > OfficialSourceMusic > OMV > UGC`).
- **Smart Multi-Key Sorting & Minimal-Move Reordering:** Composable sort keys (`artist`, `album`, `title`, `duration`, `year`, `genre`, `type`, `shuffle`) paired with a Longest Increasing Subsequence (LIS) move planner to minimize `ACTION_MOVE_VIDEO_BEFORE` API calls.
- **Set Algebra, Health Scanner & Genre Splitting:** `diff` / `intersect` / `merge`, dead/private track detection + `UGC -> ATV` upgrade lookup, and SQLite-cached MusicBrainz + fallback enrichment.
- **Interactive TUI & CLI:** `clap` CLI + `ratatui` terminal UI.

## Workspace Layout

- `crates/ytm-core` — InnerTube client, auth signing, recursive JSON parser, pagination, chunked mutations
- `crates/ytm-store` — Cross-platform paths (`ProjectDirs`), SQLite cache & snapshots, JSON/CSV/M3U8 exports
- `crates/ytm-ops` — Dedupe, LIS minimal-move sort, set algebra, playlist health scanner
- `crates/ytm-enrich` — Rate-limited metadata enrichment & `genres.toml` taxonomy splitter
- `crates/ytm-cli` — CLI binary (`ytm`) and Ratatui TUI

## Build

```bash
cargo build --release
```

## License

Dual-licensed under `MIT OR Apache-2.0`. See `LICENSE-MIT` and `LICENSE-APACHE`.
