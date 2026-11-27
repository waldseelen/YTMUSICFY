# Dalga v0.4.0 Release Notes

**Release Date:** 2026-11-27  
**Version:** 0.4.0  
**Codename:** "All of Artist & Audio Power Tools"

---

## Highlights

This release introduces comprehensive "All of Artist" harvesting, playlist quick-add, professional audio effects (EQ, speed, sleep timer), play history tracking, accessibility improvements, and optional Last.fm scrobbling.

---

## New Features

### Task 19: All of Artist Harvest
- **Harvest Engine** (`ytm-core/src/artist_harvest.rs`): Recursive artist page walk collecting albums, singles, appears-on shelves with continuations and "See all" sub-pages. TR/EN shelf classification via fixture-verified recursive tree search.
- **Dedupe Planner** (`ytm-ops/src/artist_plan.rs`): Strict ATV-over-UGC preference with `include_features` / `include_compilations` toggles and skip-existing filter. Unit tests for shelf classification, continuation walking, and option filters.
- **Apply Flow** (`crates/ytm-gui/src/views/artist_harvest.rs`): Context-menu `All of Artist` enabled when artist browse ID present. Preview modal (New Playlist / Existing Playlist picker) stages `AddTracks` with `allow_duplicates = false`. Mocked-client test asserts mandatory pre-sync SQLite snapshot → POST → read-back.

### Task 20: Playlist Quick-Add
- `favourite_playlist_id` config field + player-bar one-click `+ Favourite Playlist` button staging `AddTracks` (`allow_duplicates = false`).
- Unit test verifies staged op + snapshot/read-back.

### Task 21: Audio Power Tools
- **EQ Presets** (`crates/ytm-gui/src/audio_fx.rs`): Flat, Bass Boost, Vocal, Custom 5-band, Custom 10-band via mpv `af=equalizer` filter chains.
- **Speed Control**: 0.75x–1.5x pitch-corrected via `scaletempo`.
- **Sleep Timer**: State machine (armed → fading → paused) with injected `Instant` clock and Ribbon status.
- All with unit tests for filter strings, threshold calculation, and state transitions.

### Task 22: History & Notifications
- **22.1 Play History** (`ytm-store/src/db.rs`): SQLite `play_history` table (capped 500 entries, `played_at` index) with insert/get/cap tests. Auto-recorded on track start via worker.
- **22.2 Desktop Notifications**: `notifications_enabled` config (default `false`), stub implementation ready for `notify-rust` (Linux) / `windows-toast` (Windows). Zero network when disabled.

### Task 23: Accessibility
- **Font Scale** (`font_scale` 0.8–2.0): Applied to all text styles in `theme.rs`.
- **High Contrast** (`high_contrast` bool): WCAG-AA-safe palette override.
- **Focus Ring**: 2px accent stroke on active widgets.
- **AccessKit Readiness**: Via existing `Icon` enum tooltip labels.

### Task 24: Discovery Extras
- **24.1 Similar & Radio**: Existing `WatchNextResult` / `LoadWatchNext` infrastructure reused for "Similar songs" shelf and radio station from any track/album/artist.
- **24.2 Last.fm Scrobble Opt-In** (`crates/ytm-gui/src/lastfm.rs`): Injected-clock state machine (idle → pending → submitting → submitted/failed) with 50%/4-min threshold. Credentials stored like auth. Disabled = zero network. Unit tests for threshold calc and state transitions.

---

## Fixed
- Theme palette `high_contrast`/`font_scale` integration with egui `Visuals`
- `ScrobbleState` Default derive conflict (now uses `#[derive(Default)]` with `#[default]` variant)
- Last.fm API signature parameter lifetime issues (owned `String` params)
- `SleepTimerState` Fading variant missing `started_at` field

---

## Verification Gates (All PASS)
| Gate | Command | Status |
|------|---------|--------|
| Format | `cargo fmt --all -- --check` | ✅ PASS |
| Check | `cargo check --workspace --all-targets` | ✅ PASS |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | ✅ PASS |
| Tests | `cargo test --workspace --locked` | ✅ PASS (all tests) |
| Release Build | `cargo build --release --locked --bin ytm-gui --bin ytm` | ✅ PASS |
| Line Limit | `wc -l` (max 399 lines) | ✅ PASS |

---

## Binaries
| Binary | Platform | Size |
|--------|----------|------|
| `ytm` | Linux x86_64 | ~15 MB |
| `ytm-gui` | Linux x86_64 | ~39 MB |
| `ytm.exe` | Windows x86_64 | ~15 MB |
| `ytm-gui.exe` | Windows x86_64 | ~39 MB |

**SHA256SUMS.txt** published alongside binaries.

---

## Installation

### Linux (Arch/Arch-based)
```bash
# From release assets
tar xzf ytm-architect-v0.4.0-linux-x86_64.tar.gz
cd ytm-architect-v0.4.0
./install.sh  # or copy ytm, ytm-gui to ~/.local/bin/
```

### Windows
```powershell
# Extract zip and run ytm-gui.exe
Expand-Archive ytm-architect-v0.4.0-windows-x86_64.zip
.\ytm-architect-v0.4.0\ytm-gui.exe
```

### From Source
```bash
git clone https://github.com/waldseelen/YTMUSICFY
cd YTMUSICFY
git checkout v0.4.0
cargo build --release --locked --bin ytm-gui --bin ytm
```

---

## Configuration
New config options in `config.toml`:
```toml
favourite_playlist_id = "PL_..."      # Task 20 Quick-Add target
notifications_enabled = false         # Task 22.2 Desktop notifications
font_scale = 1.0                      # Task 23 Font scaling (0.8-2.0)
high_contrast = false                 # Task 23 High contrast mode
```

---

## Credits
Built with ❤️ using Rust, egui, mpv, and the YouTube Music InnerTube API.

**Repository:** https://github.com/waldseelen/YTMUSICFY  
**Issues:** https://github.com/waldseelen/YTMUSICFY/issues