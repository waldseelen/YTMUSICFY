# PROGRESS.md

Sole authority for dates, release status, and gate verification outputs.

---

## Current Gate Status (2026-11-27 — `v0.2.0` "Dalga" Released + Local Hotfixes Built)

- **Active Roadmap:** [TASKS.md](TASKS.md) (`v0.2.0` → `v0.5.0`; `v0.2.0` Tasks `1`–`12` complete; post-release local fixes ready for `v0.2.1` upon approval).
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace --locked`: **PASS** exit 0 (`162/162` tests: `ytm-cli` 6, `ytm-core` 33, `ytm-enrich` 14, `ytm-gui` 81, `ytm-ops` 14, `ytm-store` 14)
- `cargo build --release --locked --bin ytm-gui --bin ytm`: **PASS** exit 0
- `wc -l`: **PASS** 0 `.rs` files > 400 lines (`max = 399` in `crates/ytm-store/tests/store_integration_test.rs`)
- **Latest Published Release:** `v0.2.0` (`c0d9a70`, Linux & Windows CI run `45983802647` **success**; local working tree holds unpushed UI/sync hotfixes for `v0.2.1`).

---

## 2026-11-27 — `v0.2.0` ("Dalga") Release & Post-Release Local Fixes

- **`c0d9a70` (`v0.2.0`, CI run `45983802647`):**
  - Completed `TASKS.md` Tasks `1`–`12`: `dalga-mark.svg` / `dalga-wordmark-{dark,light}.svg` / `dalga-tray-16.{svg,png}` vectors, 53 monochrome Lucide SVG icons (`icons.rs`), Ultra-Dark & Apple-style Light themes + OS live watcher (`theme.rs`, `theme_detect.rs`, `theme_accent.rs`), TR/EN live i18n (`i18n*.rs`), top-bar redesign + Status Ribbon + collapsible Studio Tools + independent Queue/Lyrics drawer + album-art Lightbox, debounced InnerTube instant search (`SearchController` + fixture parser: 100ms suggestions / 250ms live results), one-click Browser Session Refresh, SQLite `schema v3` offline library & lyrics cache, YouTube Music links (`Ctrl+O` / `Ctrl+Shift+O`), `gl` country code uppercase normalization (`ClientConfig::normalized_gl`), 5 headless Hyprland screenshots, and full English `README.md`.
  - Verified `SHA256SUMS.txt` (`v0.2.0` assets):
    - `ytm-architect-v0.2.0-linux-x86_64.tar.gz`: `bcf1a81cb64475115417f96ff63211274ccbae2cb84e7a284636464c3e04c458`
    - `ytm` (Linux): `b6b3c106273c4b029073925dcc2adb6a0e701491808de3a1ae1ec1d515335249`
    - `ytm-gui` (Linux): `44dcc3b04c9a80c54809f563922f2dc1f543b50bc8ba3a3f4b905789ec501f6d`
    - `ytm-architect-v0.2.0-windows-x86_64.zip`: `3fc9d59012e7e06ede8424bf9ec7aa0010a7d48da4899dfdeaa24b80c6eb53a5`
    - `ytm.exe` (Windows): `8385c1a0115c36fbbf36b9f7c1cb83363cb04909d8fe52538d083162fad06758`
    - `ytm-gui.exe` (Windows): `6f6ad025fff8510b05c8f0812c0e8e3604942c27a02e4d3fffb413ca47b51643`
- **Local Uncommitted Hotfixes (Ready for `v0.2.1` upon owner commit/push):**
  - Fixed per-frame `scroll_to_me` snap-back in `sidebar.rs` and `views/inspect_rows.rs`.
  - Updated playlist creation input/button labels (`+ Create` / `+ Olustur`) to remain clickable and auto-open Sync Drawer on submit.
  - Added pre-flight deduplication/unavailable filtering (`collect_add_ids`) and `STATUS_FAILED` tail bisection (`add_tracks_resilient`) in `worker_sync_ops.rs` + `worker_sync.rs`.

---

## 2026-11-27 — Dalga Branding, Roadmap Reset & Wayland/Keyring Fixes

- **`fc69dca` — Dalga Brand & Assets (`TASKS.md` Task `2.2`–`2.4`):**
  - Created `crates/ytm-gui/src/brand.rs` centralizing `Dalga` visible strings and embedded Concept A monochrome 3D logos (`assets/brand/dalga-{dark,light}*.png`, `dalga-icon-256.png`, `dalga.ico`, `dalga.desktop`).
  - Wired window icon + title (`lib.rs`), top bar logo/label (`overlays.rs`), MPRIS display name (`media_controls.rs`), TUI title (`tui_render.rs`), and theme-aware README `<picture>` header (`README.md`).
- **`8520607` & `51d37fb` — Roadmap Reset (`TASKS.md`):**
  - Archived v0.1.x re-architecture plan in git history (`43/43` complete) and wrote `v0.2.0` → `v0.5.0` roadmap with locked decisions (`B1`–`G1`, `N1`, `K1`–`K6`, `M2` owner-approved).
- **`437f119` — Wayland ANR & Keyring Prompt Fixes:**
  - `crates/ytm-gui/src/lib.rs`: disabled blocking GL `vsync` when `WAYLAND_DISPLAY` is set (`should_enable_vsync`) to stop `eglSwapBuffers` deadlocks on hidden Hyprland workspaces.
  - `crates/ytm-gui/src/{app_events,player,player_ipc}.rs`: throttled inactive/minimized `time-pos` & status repaints (`200ms`/`500ms` → `1500ms`).
  - `crates/ytm-gui/src/auth_store.rs`: read `0600` `auth.json` directly and gate `secret-tool` behind `YTM_USE_SECRET_SERVICE=1`, eliminating GNOME Keyring modal prompts on autologin desktops.

---

## 2026-11-26 — Re-Architecture Closure & Releases (`v0.1.0` → `v0.1.1`)

- **`v0.1.1` (`cb859ea`, CI run `45852052661`):** Fixed `yt-dlp` Brave cookie extraction (`0700` `TempDir` + `about:blank` + Linux keyring/basictext fallbacks in `worker_auth.rs`) and added off-thread Wayland clipboard import (`WorkerCmd::ImportClipboardAuth` via `wl-paste`/`xclip`/`xsel`/`powershell`).
- **`v0.1.0` (`1a8bb22` / `8060540`, CI run `45839060451`):** Completed Phases 0–5 (`43/43` tasks) across all 6 crates, fixed Windows `souvlaki` SMTC `HWND` init (`media_controls.rs`), and published initial Linux + Windows binaries with `SHA256SUMS.txt`.
