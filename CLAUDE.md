# CLAUDE.md

Binding engineering rules and operational invariants for **Dalga** (workspace `ytm-architect`), a local-first cross-platform (Windows & Linux) native music & media desktop player and library engineering suite in pure Rust.

---

## 0. Required Read Order

1. `CLAUDE.md` — binding rules and verification gates
2. `ARCHITECTURE.md` — 6-crate workspace topology, file locator, and data flow
3. `SPEC.md` — domain schemas, algorithms, and CLI contracts
4. `MEMORY.md` — durable InnerTube and Rust traps
5. `PROGRESS.md` — sole owner of dates, release status, and gate verification outputs
6. `README.md` — user setup, releases, and CLI quickstart
7. `TASKS.md` — active product roadmap (`v0.2.0` → `v0.5.0`), locked product decisions, and verification gates per task

---

## 1. Non-Negotiable Engineering Rules

1. **Contract-First, Wave-Based Execution:**
   - Do not write crate implementations until Wave 0A reference audit (`.refs/RESEARCH_NOTES.md`, `tests/fixtures/`) and Wave 0B contract lock (`crates/*/src/{models,traits,error}.rs`) compile cleanly.
   - Maintain strict non-overlapping file ownership across crate boundaries.
2. **Mandatory Verification Gates:**
   - Every wave must pass:
     - `cargo fmt --all -- --check`
     - `cargo check --workspace --all-targets`
     - `cargo clippy --workspace --all-targets -- -D warnings`
     - `cargo test --workspace`
3. **Zero-Placeholder Policy:**
   - Never commit `todo!()`, `unimplemented!()`, or silent stubs in production paths.
   - No source file may exceed 400 lines; split modules when approaching this limit.
   - Never suppress lints (`#[allow(warnings)]`, `#[allow(clippy::all)]`).
4. **Safety-First Remote Mutations (`--dry-run` + Pre-Flight Snapshots):**
   - Mutating commands (`dedupe`, `sort`, `split-genre`, `set --into`, `health --prune-unavailable/--upgrade`, `snapshots restore`) MUST default to dry-run preview unless `--execute` is passed explicitly.
   - Before any mutating HTTP POST (`browse/edit_playlist`, `playlist/create`, `playlist/delete`), persist an atomic pre-mutation snapshot (`ytm-store`) first.
5. **Anti-Fragile InnerTube JSON Parsing:**
   - Never rely on brittle hardcoded array index chains (`json["contents"][0]...`) for `youtubei/v1` payloads.
   - Use recursive JSON tree-search helpers (`find_all_by_key`, `find_first_by_key`, `extract_runs_text`) verified against sanitized JSON fixtures in `tests/fixtures/`.
6. **Cross-Platform First-Class Support (Windows & Linux):**
   - Resolve all config, cache, and snapshot paths via `directories::ProjectDirs::from("io", "ytm-architect", "ytm-architect")`. Never hardcode `~/.config`, `~/.local/share`, or platform-specific separators.
   - Configure `reqwest` with `default-features = false` and `rustls-tls` (zero system OpenSSL dependency).
   - Set `0600` permissions on `auth.json` on Unix (`#[cfg(unix)]`); emit user-scoped ACL notice on Windows (`#[cfg(windows)]`).
7. **Hygiene & Licensing:**
   - Write original Rust code under `MIT OR Apache-2.0`. Study `.refs/` for protocol shapes and edge cases only; never copy third-party code verbatim.
   - Keep `.refs/`, `*.sqlite*`, `auth.json`, `browser.json`, and `headers_auth.json` gitignored.
8. **No Unapproved Remote CI / Git Push Triggers:**
   - Never run `git push`, tag pushes, or remote GitHub Actions workflows (`gh workflow run` or pushing tags/branches that trigger CI) without the user's explicit per-action approval immediately beforehand; remote CI runners consume repository quota.

---

## 2. Document Hierarchy (One Fact, One Home)

| Document | Sole Ownership | Forbidden Content |
| :--- | :--- | :--- |
| `AGENT.md` | Pointer to `CLAUDE.md` and read order | Rules, dates, checkboxes |
| `CLAUDE.md` | Binding rules, safety invariants, verification gates | Dates, checkboxes, session logs |
| `ARCHITECTURE.md` | Workspace crate tree, data flow, external services | Dates, checkboxes, roadmaps |
| `SPEC.md` | Schemas, algorithms, CLI & InnerTube contracts | Dates, checkboxes, speculative roadmaps |
| `MEMORY.md` | Durable InnerTube & system traps | Dates, checkboxes, version pins |
| `PROGRESS.md` | Sole owner of dates, releases, and gate outputs | Static rules or specs |
| `README.md` | Setup, credential import, releases, CLI usage | Agent rules, gate logs |
| `TASKS.md` | Active `v0.2.0–v0.5.0` roadmap, product decisions, task checklists | Gate logs, implementation prose |
