# ARCHITECTURE.md

## Workspace Topology

```text
ytm-architect/
├── Cargo.toml                  # Workspace manifest & shared dependencies
├── config/
│   ├── config.example.toml     # Client context, pacing, enrichment provider settings
│   └── genres.example.toml     # Hierarchical subgenre -> parent genre rules
├── crates/
│   ├── ytm-core/               # Auth signing, InnerTube HTTP + MockTransport, JSON tree parser, pagination, chunked mutations
│   ├── ytm-store/              # ProjectDirs paths, SQLite cache + migrations, atomic snapshots, JSON/CSV/M3U8 export/import
│   ├── ytm-ops/                # Normalization, exact/fuzzy deduper, multi-key sorter + LIS minimal-move planner, set algebra, health scanner
│   ├── ytm-enrich/             # Rate-limited MusicBrainz + optional Last.fm/Deezer/ListenBrainz + genres.toml mapper + split-genre planner
│   └── ytm-cli/                # Clap CLI, comfy-table dry-run diff renderer, indicatif progress, Ratatui TUI
└── tests/
    ├── fixtures/               # Sanitized InnerTube JSON response payloads for deterministic tests
    └── integration/            # End-to-end mocked pipeline & dry-run/snapshot roundtrip tests
```

## Crate Dependency Graph & Layer Flow

```text
ytm-cli (binary + TUI)
  ├── ytm-ops    (pure domain algorithms: dedupe, LIS sort, set algebra, health scan)
  ├── ytm-enrich (metadata cache + rate-limited external APIs -> ytm-store, ytm-core)
  ├── ytm-store  (SQLite persistence, atomic snapshots, JSON/CSV/M3U8 exports -> ytm-core models)
  └── ytm-core   (InnerTube HTTP transport, SAPISIDHASH auth, recursive JSON parser)
```

- **`ytm-core`** depends on zero sibling workspace crates. Exposes `InnerTubeClientTrait` and `HttpTransport` so all upstream crates and integration tests mock InnerTube deterministically without network access.
- **`ytm-store`** depends only on `ytm-core` domain models (`Playlist`, `Track`, `MusicVideoType`). Owns OS path resolution (`directories::ProjectDirs`), SQLite schema migrations, and atomic pre-mutation snapshots.
- **`ytm-ops`** depends only on `ytm-core` domain models. Contains pure, deterministic planning functions (`plan_dedupe`, `plan_sort`, `plan_set_op`, `scan_health`) with zero network or filesystem I/O.
- **`ytm-enrich`** depends on `ytm-core` and `ytm-store` (for `EnrichmentCacheStore` & `HttpTransport` or provider HTTP abstraction). Checks local SQLite cache before querying external providers.
- **`ytm-cli`** wires CLI subcommands, enforces `--dry-run` default, executes pre-mutation snapshots through `ytm-store` before calling mutating methods on `InnerTubeClientTrait`, and hosts the `ratatui` interactive dashboard.

## External Services

| Service | Purpose | Pacing / Constraints |
| :--- | :--- | :--- |
| YouTube Music InnerTube (`music.youtube.com/youtubei/v1/`) | Library browse, pagination, playlist mutations, search | `SAPISIDHASH`, max 50 actions/POST, 600ms delay, jittered backoff on `429`/`503` |
| MusicBrainz (`/ws/2/recording`, `/ws/2/artist`) | Primary canonical genre/tag & release-year enrichment | `>= 1.1s` interval per request, compliant `User-Agent` |
| Last.fm / Deezer / ListenBrainz (optional) | Fallback genre & release metadata when MusicBrainz misses | Enabled via `config.toml` or env vars; cached in SQLite |
