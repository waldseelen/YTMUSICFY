# PROGRESS.md

Sole authority for dates, release status, and gate verification outputs.

---

## Current Gate Status (2026-11-27 — Dalga Brand Locked & Runtime Hardened)

- **Active Roadmap:** [TASKS.md](TASKS.md) (`v0.2.0` → `v0.5.0`, **Dalga** product roadmap; `v0.2.0` Task `2.2`–`2.4` complete, `2.1` draft approved).
- `cargo fmt --all -- --check`: **PASS** exit 0
- `cargo check --workspace --all-targets`: **PASS** exit 0, 0 warnings
- `cargo clippy --workspace --all-targets -- -D warnings`: **PASS** exit 0, 0 warnings
- `cargo test --workspace`: **PASS** exit 0 (`131/131` tests: `ytm-cli` 6, `ytm-core` 31, `ytm-enrich` 14, `ytm-gui` 54, `ytm-ops` 14, `ytm-store` 12)
- `cargo build --release --bin ytm-gui --bin ytm`: **PASS** exit 0 (`7.63s`)
- `wc -l`: **PASS** 0 `.rs` files > 400 lines (`max = 400` in `crates/ytm-gui/src/worker.rs`)
- **Latest Published Release:** `v0.1.1` (`cb859ea`, Linux & Windows CI run `45852052661` **success**; local `master` is ahead with runtime fixes + roadmap + Dalga branding).

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
