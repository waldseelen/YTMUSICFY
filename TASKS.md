# TASKS.md — YTMuse Product Roadmap (v0.2.0 → v0.5.0)

Execution plan for turning the `ytm-architect` engineering suite into **YTMuse**, an end-user native YouTube Music desktop client (Rust + egui, Linux & Windows). The v0.1.0/v0.1.1 re-architecture plan (Phases 0–5, 43/43 tasks) is complete and archived in git history (checkpoint commit before this file).

Legend: `[ ]` open · `[x]` done. Every task ends with `verify:` checks that must pass before the box is ticked. Dates and gate outputs live only in `PROGRESS.md`.

---

## 0. Locked Decisions

| ID | Decision |
| :--- | :--- |
| B1 | **Brand:** visible name is **YTMuse** (tagline `YTMuse for YouTube Music`). Applies to window title, app id display name, tray, icons, README. Binary names `ytm-gui` / `ytm` and crate names stay unchanged. |
| B2 | **Logo set:** two monochrome 3D-perspective logos (dark-theme and light-theme variants), one horizontal wordmark, one 1024px app icon. Generated drafts are shown to the owner before being embedded. |
| B3 | **README:** centered logo via `<picture>` (auto light/dark on GitHub), badges (version, license, platform), real screenshots (Home, Search, Mini Player, light/dark side by side), feature table, download + SHA-256 verification, short "Why YTMuse" section. |
| T1 | **Theme:** `ThemeMode = Light | Dark | System`. System follows OS (Linux XDG portal / `gsettings`, Windows registry). Dynamic accent from album art dominant colour over static Ultra-Dark / Apple-style Light surfaces. Selectable accents: Gold, YTM Pink, Emerald. |
| T2 | **Language:** Turkish + English, switchable in Settings, default = system language else English. |
| T3 | **Icons:** emoji and font-dependent Unicode glyphs are banned in UI. All icons are embedded Lucide SVGs rendered through `egui_extras` SVG loader and tinted by theme. |
| U1 | **Layout:** top bar = Back/Forward + global search + theme/accent + Mini Player + Settings/Profile; left = Library + collapsible Studio Tools; bottom player bar with separate **Lyrics** and **Queue** buttons opening a right drawer; album-art **Lightbox** closes on outside click or `Esc`. Developer clutter (`Backup All`, shortcut hint strip) leaves the main chrome. |
| U2 | **Status Ribbon:** right side of top bar, right-to-left ticker. Priority: error/warning (sticky until dismissed) > action result (4–6 s) > contextual hint > rare time-of-day greeting (max 1/hour, never while playing in focus). Settings: disable greetings, scroll speed. Errors and action results cannot be disabled. Click opens last 50 messages. |
| S1 | **Session refresh:** one-click "Refresh Session" opens `music.youtube.com/watch?v=<song>` in the system browser, waits, imports cookies via `yt-dlp`, shows a toast. Expired session at startup shows a banner offering it. Settings toggle enables fully automatic refresh (amends old D3: silent refresh stays OFF by default). |
| Q1 | **Search:** layer 1 (~100 ms) YouTube Music suggestions + recent queries; layer 2 (250 ms debounce) live dropdown (Top match, 3 songs, artist, album); `Enter` opens full results with filter tabs. Stale requests are cancelled. Mini Player reuses the same engine (top 3). |
| Y1 | **Jump to YouTube:** context menu / player bar / palette actions: Open in YouTube Music, Search on YouTube, Copy link, Open at current second (`&t=`). `Ctrl+O` / `Ctrl+Shift+O`. Default browser, optionally chosen in Settings. |
| M1 | **Tray + Mini Player:** tray icon (theme-aware) toggles a borderless, draggable, always-on-top Mini Player with Compact ↔ Expanded modes (transport, shuffle, repeat, like, search with top 3 results). Closing the main window with tray enabled keeps playback running; full quit via tray menu. |
| M2 | **Implementation calls (owner-approved, no veto):** Linux tray via `ksni` (StatusNotifierItem, no GTK); Windows tray via `tray-icon`; Mini Player = egui secondary viewport; rotating phrases are an embedded TR/EN list (no network, no telemetry). |
| P1 | **Offline-first cache:** playlists, library, artwork, lyrics persisted; stale-while-revalidate on startup; library browsable offline, only playback needs network. |
| P2 | **All of Artist keeps D1 safety:** harvest → dedupe → preview → `SyncQueue` → verified POST. |
| R1 | **Releases:** v0.2.0 (look, session, README) → v0.3.0 (tray, Mini Player, Home shelves, shortcuts) → v0.4.0 (power features, accessibility). Each ships Linux + Windows artifacts with `SHA256SUMS.txt`. |
| N1 | **Name is provisional.** `YTMuse` stays through v0.2.0. Owner may rename to a phonetic, Turkish-meaning, ASCII-spelled name (candidates: Ezgi, Ahenk, Sada, Yanki, Nagme); all brand strings therefore live in one constants module (Task 2.4). GitHub/crates.io collision check is run before any rename; trademark search is out of scope. |
| K1 | **Multi-source player (v0.5.0):** YouTube Music, Radio, Local Files, Video are *sources*; YouTube Music is one of them. |
| K2 | **Radio:** bundled 551 Turkish stations (from `waldseelen/RadioStack` `stations.m3u`), user M3U/M3U8/XSPF import/export, optional Radio Browser directory search; favourites, categories, ICY Now Playing, auto-retry, offline badge. Local SQLite only, no server/Firebase. |
| K3 | **Video:** mpv in a separate borderless video window (YT Music video version, local video files); player bar, queue and MPRIS unchanged, no double playback. Embedded libmpv render is deferred to a later release after risk measurement. |
| K4 | **Local library:** watched folders, tags + embedded covers via `lofty` indexed into SQLite, unified search box, drag-and-drop open, "Open with" registration, local playlists with M3U8 export (never mixed with YT Music playlists). |
| K5 | **Source abstraction first:** after v0.4.0, `MediaSource { YouTube{video_id}, Radio{station_id,url}, LocalFile{path} }` + `PlayableItem` drive player, queue, session restore and MPRIS. `Track` stays YouTube-specific; saved sessions stay backward compatible via serde defaults. |
| K6 | **Dependencies not bundled:** mpv, yt-dlp, ffmpeg are detected by a first-run "System Check" screen (found/missing badges, per-platform one-line install command for pacman/apt/winget/scoop, Recheck). README documents the official Windows mpv download and SHA-256. |
| G1 | **Inherited gates (unchanged):** `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, no `.rs` file > 400 lines, zero new `#[allow(...)]`, no emoji in UI strings. |

---

## Phase v0.2.0 — "YTMuse": Brand, Theme, Chrome, Session

### Task 1 — Contract lock & docs
- [ ] **1.1 Domain contracts**
  - [ ] Add `ThemeMode`, `AccentPalette`, `UiLang`, `StatusMessage`, `StatusPriority` enums/structs with serde in `ytm-gui` (no parallel bool flags)
  - [ ] Persist new fields in `GuiAppConfig` with `#[serde(default)]` so old `config.toml` still loads
  - [ ] verify: unit test loads a v0.1.1 `config.toml` fixture and yields defaults
- [ ] **1.2 Docs sync**
  - [ ] Update `SPEC.md`, `ARCHITECTURE.md`, `GLOSSARY.md` for YTMuse terms
  - [ ] verify: `rg -n "ytm-architect" README.md SPEC.md` lists only intentional binary/crate references

### Task 2 — Brand & logo
- [ ] **2.1 Logo generation**
  - [ ] Generate dark-variant and light-variant monochrome 3D logos, wordmark, 1024px icon via `image-generator`
  - [ ] Present drafts to owner; iterate until approved
  - [ ] verify: owner approval recorded in `PROGRESS.md`
- [ ] **2.2 Asset packaging**
  - [ ] Store sources under `assets/brand/`; export PNG sizes, `.ico` (Windows), `.desktop` + hicolor icons (Linux)
  - [ ] Embed window icon via `ViewportBuilder::with_icon`
  - [ ] verify: `file assets/brand/*.png` dimensions match; app launches with icon on Linux
- [ ] **2.3 Rename visible strings**
  - [ ] Window title, top-bar label, About, tray title → `YTMuse`
  - [ ] verify: `rg -n '"ytm-architect"' crates/ytm-gui/src` returns only app-id/config-path constants
- [ ] **2.4 Central brand constants (N1)**
  - [ ] `brand.rs` exposes `APP_NAME`, `TAGLINE`, `WINDOW_TITLE`, tray title; every visible brand string reads from it
  - [ ] verify: unit test asserts no other module contains the literal `"YTMuse"` (`rg -n 'YTMuse' crates/ytm-gui/src` hits only `brand.rs` and tests)

### Task 3 — Theme engine
- [ ] **3.1 Palettes**
  - [ ] Replace constants in `theme.rs` with `Palette` structs for Ultra-Dark and Light surfaces
  - [ ] Accent palettes: Gold, YTM Pink, Emerald
  - [ ] verify: unit test asserts WCAG AA contrast (>= 4.5) for text on each surface
- [ ] **3.2 System theme detection**
  - [ ] Linux: XDG desktop portal `color-scheme` with `gsettings` fallback; Windows: registry `AppsUseLightTheme`
  - [ ] Live-follow OS changes when mode is `System`
  - [ ] verify: unit tests for parser functions (portal value 0/1/2, registry 0/1); manual toggle on Hyprland
- [ ] **3.3 Dynamic album accent**
  - [ ] Extract dominant colour off the UI thread (downscaled, quantized), clamp lightness for contrast
  - [ ] Smooth transition; fall back to selected accent
  - [ ] verify: unit test on fixture images returns stable colour; frame time unaffected (no work in `update`)
- [ ] **3.4 Theme controls**
  - [ ] Top-bar quick switch + Settings page (mode, accent, dynamic accent toggle)
  - [ ] verify: restart restores selection; clippy + tests green

### Task 4 — Icon system (Lucide)
- [ ] **4.1 SVG pipeline**
  - [ ] Vendor required Lucide SVGs under `assets/icons/`; enable `egui_extras` `svg` feature; `icons.rs` exposes typed `Icon` enum
  - [ ] Tint by theme colour, cache textures
  - [ ] verify: unit test every `Icon` variant maps to an existing embedded file
- [ ] **4.2 Replace glyphs**
  - [ ] Replace every `✓ ✕ ♥ ⚠ ▶ ⏸ ⇄ ⋮⋮ ↻ ⇅ ⊘` and text mute labels across `views/*`, `overlays.rs`, `navigation.rs`, `play_queue.rs`, `interaction.rs`
  - [ ] verify: `rg -n "[✓✕♥♡⚠▶◀⏸⏹⏮⏭⇄↻⇅⊘♪]" crates/ytm-gui/src --glob '!*_tests.rs'` returns nothing

### Task 5 — i18n (TR/EN)
- [ ] **5.1 String catalog**
  - [ ] `i18n` module with `Key` enum and static TR/EN tables; `tr!(Key)` lookup
  - [ ] verify: test asserts every `Key` has non-empty TR and EN entries
- [ ] **5.2 Migrate UI strings**
  - [ ] Move top bar, sidebar, player bar, Settings, Auth, toasts to catalog
  - [ ] verify: switching language updates UI without restart; `rg` finds no hard-coded English button labels in migrated views

### Task 6 — Application chrome
- [ ] **6.1 Top bar redesign**
  - [ ] Back/Forward, centered search box, theme/accent, Mini Player placeholder, profile/settings; remove `Backup All`, `Refresh Library`, shortcut strip (moved to menus/palette)
  - [ ] verify: screenshot compare in light and dark; `Ctrl+K` palette still lists moved actions
- [ ] **6.2 Studio Tools menu**
  - [ ] Collapsible sidebar section containing Dedupe, Sort, Set Ops, Health, Genre Split, Snapshots, Sync
  - [ ] verify: every former workspace tab reachable; navigation history tests pass
- [ ] **6.3 Lyrics & Queue drawer**
  - [ ] Separate player-bar buttons; right drawer with tabs; lyrics usable without opening queue
  - [ ] verify: unit test for drawer state machine (`Closed | Queue | Lyrics`); manual: lyrics visible with queue closed
- [ ] **6.4 Album-art Lightbox**
  - [ ] Click cover → large modal image; click outside or `Esc` closes
  - [ ] verify: unit test for open/close reducer; manual outside-click and `Esc`

### Task 7 — Status Ribbon
- [ ] **7.1 Message bus**
  - [ ] `StatusBus` with priority queue, sticky errors, TTL for results, history ring (50)
  - [ ] Route existing `ToastState` producers and `WorkerEvent::StatusToast` into it
  - [ ] verify: unit tests for priority ordering, sticky-until-dismiss, ring eviction
- [ ] **7.2 Ticker widget**
  - [ ] Right-to-left scroll with speed setting; click opens history popup; reduced-motion = static fade
  - [ ] verify: unit test for scroll offset math with injected clock; manual visual check
- [ ] **7.3 Greetings**
  - [ ] Embedded TR/EN phrase lists by time of day; rate limit 1/hour; suppressed while playing and focused; Settings toggle
  - [ ] verify: unit test with injected clock/RNG covers rate limit and suppression

### Task 8 — Instant search engine
- [ ] **8.1 Core endpoints**
  - [ ] `ytm-core`: `get_search_suggestions` (`music/get_search_suggestions`) with fixture-tested parser
  - [ ] verify: fixture test in `ytm-core/tests`
- [ ] **8.2 Search controller**
  - [ ] Debounce 100 ms suggestions / 250 ms live results, request IDs to drop stale responses, LRU recent queries persisted
  - [ ] verify: unit tests with fake client: stale response ignored, debounce coalesces keystrokes
- [ ] **8.3 Dropdown UI**
  - [ ] Top match + 3 songs + artist + album; keyboard navigation; `Enter` opens full results with filter tabs
  - [ ] verify: manual keyboard walkthrough; measured p50 suggestion latency logged in `PROGRESS.md`

### Task 9 — Session refresh (S1)
- [ ] **9.1 Browser launcher**
  - [ ] Cross-platform `open_url` (xdg-open / `start`), configurable browser; builds `watch?v=` URL
  - [ ] verify: unit test for URL builder; command construction tested without spawning
- [ ] **9.2 Refresh flow**
  - [ ] "Refresh Session" button + startup banner; wait ~6–8 s, run browser-cookie import, toast result via Status Ribbon
  - [ ] Settings toggle for fully automatic refresh (default OFF)
  - [ ] verify: unit tests for state machine (`Idle | Opening | Importing | Done | Failed`); manual end-to-end on Brave
- [ ] **9.3 Keyring noise**
  - [ ] Confirm `YTM_USE_SECRET_SERVICE` opt-in and Wayland no-vsync fixes remain covered by tests
  - [ ] verify: existing tests `secret_service_is_opt_in...` and `vsync_disabled_on_linux_wayland_only` pass

### Task 10 — Offline-first cache (P1)
- [ ] **10.1 Persisted caches**
  - [ ] Lyrics + library summaries stored in SQLite alongside playlists; versioned schema migration
  - [ ] verify: store integration test round-trips and migrates an old DB fixture
- [ ] **10.2 Stale-while-revalidate**
  - [ ] Paint from cache first; refresh in background; offline banner when network fails
  - [ ] verify: test with failing client still yields cached library state

### Task 11 — Jump to YouTube (Y1)
- [ ] **11.1 Link builder**
  - [ ] Pure functions for YT Music URL (track/album/artist/playlist), YouTube search URL, `&t=` variant
  - [ ] verify: table-driven unit tests with URL-encoding edge cases
- [ ] **11.2 Wiring**
  - [ ] Context menu, player bar, palette actions; `Ctrl+O`, `Ctrl+Shift+O`
  - [ ] verify: manual open in default browser; shortcut test in `interaction.rs`

### Task 12 — README & release v0.2.0
- [ ] **12.1 Screenshots**
  - [ ] Capture Home, Search, Lightbox, light/dark pair into `assets/screenshots/`
  - [ ] verify: files referenced by README exist (`rg -o "assets/[^)\" ]+" README.md | sort -u | xargs ls`)
- [ ] **12.2 README rewrite (English)**
  - [ ] Hero `<picture>` logo, badges, features table, install + SHA-256 verification, usage, "Why YTMuse"
  - [ ] verify: GitHub renders light/dark logos (check after push); links resolve
- [ ] **12.3 Phase gate & release**
  - [ ] Bump to `0.2.0`; run G1 gates; tag `v0.2.0`; CI builds Linux + Windows; publish with `SHA256SUMS.txt`; write hashes into README + `PROGRESS.md`
  - [ ] verify: `gh run view` success for both jobs; `gh release view v0.2.0 --json assets`

---

## Phase v0.3.0 — Tray, Mini Player, Home, Shortcuts

### Task 13 — Window lifecycle & tray (M1)
- [ ] **13.1 Tray abstraction**
  - [ ] `TrayBackend` trait; Linux `ksni` implementation, Windows `tray-icon` implementation, no-op fallback when no tray host
  - [ ] verify: trait tests with fake backend; build passes on both targets in CI
- [ ] **13.2 Tray menu & icon**
  - [ ] Theme-aware icon; menu: Show/Hide, Play/Pause, Next, Previous, Mini Player, Quit
  - [ ] verify: manual on Waybar (Linux); Windows verified in CI-built binary by owner
- [ ] **13.3 Close-to-tray**
  - [ ] Intercept window close when tray active; playback continues; quit only from tray/menu; Settings toggle
  - [ ] verify: unit test for close-request reducer (`Quit | HideToTray`)

### Task 14 — Mini Player
- [ ] **14.1 Secondary viewport**
  - [ ] Borderless, always-on-top, draggable; position/size persisted per monitor
  - [ ] verify: unit test for position clamping to visible monitors; manual drag on Hyprland (floating rule documented)
- [ ] **14.2 Compact mode**
  - [ ] Cover, title/artist, transport, like, shuffle, repeat, progress
  - [ ] verify: state-sync test: actions in Mini Player reach `PlayerController` and reflect back
- [ ] **14.3 Expanded mode**
  - [ ] Search field reusing Task 8 controller; top 3 results; click plays; collapse back
  - [ ] verify: unit test that Mini Player and main search share one controller instance
- [ ] **14.4 Spotify-style behaviors**
  - [ ] Snap to screen edges, opacity-on-idle option, resize handles
  - [ ] verify: unit tests for snap math

### Task 15 — Home shelves & Continue card
- [ ] **15.1 Home parser**
  - [ ] Parse personalised shelves (Listen Again, Mixes, New Releases) with fixtures
  - [ ] verify: `ytm-core` fixture tests, recursive-search helpers only (no index chains)
- [ ] **15.2 Shelf UI**
  - [ ] Card grid with horizontal scroll, skeleton loading, "Continue" card from saved playback session
  - [ ] verify: manual; cache-first paint verified offline

### Task 16 — Consistent context menu
- [ ] **16.1 Menu model**
  - [ ] One `ContextTarget` enum (Track, Album, Artist, Playlist) → one menu builder: queue next/last, radio, add to playlist, share link, go to artist, All of Artist (disabled until v0.4.0), Jump to YouTube
  - [ ] verify: table test: each target yields the expected action set
- [ ] **16.2 Adopt everywhere**
  - [ ] Replace per-view menus in Inspector, Search, Browse, Queue, Home
  - [ ] verify: `rg -n "context_menu" crates/ytm-gui/src/views` shows only the shared builder

### Task 17 — Keyboard-first
- [ ] **17.1 Shortcut registry**
  - [ ] Central table of shortcuts (media keys, `Space`, `Ctrl+K`, `/` focus search, arrows, `Ctrl+O`, mini-player toggle)
  - [ ] verify: registry test detects duplicate bindings
- [ ] **17.2 Shortcuts page**
  - [ ] Settings page listing shortcuts generated from the registry
  - [ ] verify: page content equals registry (test)

### Task 18 — Release v0.3.0
- [ ] **18.1 Gates & docs**
  - [ ] G1 gates; update README features + screenshots (Mini Player, tray); `PROGRESS.md`
  - [ ] verify: all G1 commands exit 0
- [ ] **18.2 Publish**
  - [ ] Bump `0.3.0`, tag, CI Linux + Windows, `SHA256SUMS.txt` in README
  - [ ] verify: `gh release view v0.3.0 --json assets`

---

## Phase v0.4.0 — Power Features & Accessibility

### Task 19 — All of Artist (P2)
- [ ] **19.1 Harvest**
  - [ ] Fetch artist page → albums, singles, appears-on (with continuation); collect all tracks
  - [ ] verify: fixture tests for artist browse + continuation
- [ ] **19.2 Plan**
  - [ ] Pure `ArtistHarvestPlan` in `ytm-ops`: dedupe (ATV preferred over UGC), options (include features/compilations)
  - [ ] verify: unit tests in `ytm-ops` for dedupe priority and option filters
- [ ] **19.3 Apply**
  - [ ] Preview dialog → new or existing playlist → `SyncQueue` → verified POST with snapshot
  - [ ] verify: mocked-client test asserts snapshot precedes mutation and read-back runs

### Task 20 — Playlist Quick-Add
- [ ] **20.1 Target list**
  - [ ] Player-bar one-click add to chosen favourite playlist; picker in Settings
  - [ ] verify: unit test stages `AddTracks` with `allow_duplicates = false`

### Task 21 — Audio power tools
- [ ] **21.1 EQ presets**
  - [ ] mpv `af=equalizer` chains: Flat, Bass Boost, Vocal, Custom (5/10-band)
  - [ ] verify: unit tests for filter-string builder; manual audible check
- [ ] **21.2 Speed**
  - [ ] 0.75x–1.5x via mpv `speed`, pitch-corrected
  - [ ] verify: IPC command test with fake runtime
- [ ] **21.3 Sleep timer**
  - [ ] Timer with injected clock; fade-out then pause; status via Ribbon
  - [ ] verify: unit tests for timer state machine

### Task 22 — History & notifications
- [ ] **22.1 Recently played**
  - [ ] Persist play history in SQLite; quick-access list on Home
  - [ ] verify: store test for insert/cap/ordering
- [ ] **22.2 Desktop notifications**
  - [ ] Optional OS notification on track change (Linux `notify-rust`/D-Bus, Windows toast); off by default
  - [ ] verify: disabled-by-default test performs zero notification calls (G-rule 3.6)

### Task 23 — Accessibility
- [ ] **23.1 Scale & contrast**
  - [ ] Font scale setting, high-contrast palette, visible focus ring
  - [ ] verify: contrast unit tests reused from Task 3.1
- [ ] **23.2 Screen reader**
  - [ ] Enable AccessKit; label icon-only buttons through the `Icon` enum tooltips
  - [ ] verify: test asserts every icon button has a non-empty label

### Task 24 — Discovery extras
- [ ] **24.1 Similar & radio**
  - [ ] "Similar songs" from watch-next; radio from any target
  - [ ] verify: fixture parser tests
- [ ] **24.2 Last.fm scrobble (opt-in)**
  - [ ] Scrobble after 50% / 4 min with injected clock; credentials stored like auth; disabled = zero network
  - [ ] verify: unit tests for scrobble threshold; disabled-path test makes no HTTP call

### Task 25 — Release v0.4.0
- [ ] **25.1 Full verification**
  - [ ] G1 gates; manual regression of v0.2.0/v0.3.0 flows; performance notes (startup paint, search latency) in `PROGRESS.md`
  - [ ] verify: all G1 commands exit 0
- [ ] **25.2 Publish**
  - [ ] Bump `0.4.0`, README final polish + screenshots, tag, CI Linux + Windows, `SHA256SUMS.txt`
  - [ ] verify: `gh release view v0.4.0 --json assets`

---

## Phase v0.5.0 — Multi-Source Player: Radio, Local Files, Video

### Task 26 — Source abstraction (K5)
- [ ] **26.1 Domain model**
  - [ ] `MediaSource` enum and `PlayableItem` (title, artist, duration, artwork, source) in `ytm-gui` (or a shared module); `From<&Track>` for YouTube
  - [ ] verify: unit tests for conversions and serde round-trip
- [ ] **26.2 Player & queue migration**
  - [ ] `PlayerController`/actor load URL or path from `MediaSource` (no hard-coded `watch_url`); `PlayQueue` holds `PlayableItem`
  - [ ] verify: existing player/queue tests green; new fake-runtime test plays a `LocalFile` and a `Radio` item
- [ ] **26.3 Session & MPRIS**
  - [ ] Playback session snapshot gains `source` with `#[serde(default)]` YouTube fallback; MPRIS metadata per source
  - [ ] verify: test loads a v0.4.0 session JSON fixture and restores it unchanged

### Task 27 — Radio
- [ ] **27.1 Station store**
  - [ ] SQLite tables for stations, categories, favourites; migration from empty
  - [ ] Seed 551 stations from RadioStack `stations.m3u` (embedded asset, license/source noted in README)
  - [ ] verify: store test seeds and counts 551 stations across 15 categories
- [ ] **27.2 M3U/M3U8/XSPF I/O**
  - [ ] Parser and exporters with fixtures (malformed lines, duplicates, relative URLs)
  - [ ] verify: round-trip tests; duplicate URLs are merged
- [ ] **27.3 Radio Browser (opt-in)**
  - [ ] Directory search by name/country/tag with mirror selection and timeouts; disabled = zero network calls
  - [ ] verify: fixture parser tests; disabled-path test performs no HTTP call
- [ ] **27.4 Playback & Now Playing**
  - [ ] Play via mpv; read `metadata/icy-title` over IPC into Status/player bar; auto-retry with backoff; offline badge after N failures
  - [ ] verify: unit tests for ICY parsing and retry state machine with injected clock
- [ ] **27.5 Radio UI**
  - [ ] Sidebar "Radio" section: categories, favourites, search, import/export, station logo cache
  - [ ] verify: manual playback of 3 stations on Linux; keyboard navigation works

### Task 28 — Local library (K4)
- [ ] **28.1 Scanner & tags**
  - [ ] Watched folders setting; background scan with `lofty` (ID3/FLAC/Vorbis/MP4); embedded cover extraction into artwork cache
  - [ ] verify: fixture audio files yield expected tags; unreadable files are reported, not fatal
- [ ] **28.2 Index & watch**
  - [ ] SQLite index (path, mtime, tags); incremental rescans and file-change watching (`notify`); removed files pruned
  - [ ] verify: temp-dir test: add, modify, delete file updates the index
- [ ] **28.3 Views & search**
  - [ ] Albums/Artists/Songs views; local results appear in the global search dropdown under a "Local" group
  - [ ] verify: search controller test returns local hits without network
- [ ] **28.4 Open & playlists**
  - [ ] Drag-and-drop file/folder open, desktop "Open with" entry, local playlists with M3U8 export
  - [ ] verify: unit test for drop-path expansion; exported M3U8 re-imports identically

### Task 29 — Video window (K3)
- [ ] **29.1 Video mode**
  - [ ] Spawn/reuse mpv video output as a separate borderless window; toggle "Video" on player bar for YT Music tracks that have a video; local video files open the same way
  - [ ] verify: command-builder unit tests (flags per mode); manual playback with controls and queue in sync
- [ ] **29.2 Safety**
  - [ ] Single mpv instance; closing the video window returns to audio-only without stopping the track; no double audio
  - [ ] verify: fake-runtime test for mode switch preserving position and pause state

### Task 30 — System check & first run (K6)
- [ ] **30.1 Dependency detector**
  - [ ] Detect `mpv`, `yt-dlp`, `ffmpeg` (path + version) per platform; pure parser tests
  - [ ] verify: unit tests with fake `--version` outputs
- [ ] **30.2 System Check screen**
  - [ ] Found/missing badges, per-platform install command with copy button, Recheck; shown on first run and from Settings
  - [ ] verify: manual with `PATH` stripped of `yt-dlp` shows the missing badge and correct command
- [ ] **30.3 Docs**
  - [ ] README: dependency section, official Windows mpv link and SHA-256 note
  - [ ] verify: links resolve (`curl -sI` returns 200/302)

### Task 31 — Release v0.5.0
- [ ] **31.1 Verification**
  - [ ] G1 gates; regression of v0.2.0–v0.4.0 flows; radio/local/video manual matrix on Linux; Windows build smoke by owner
  - [ ] verify: all G1 commands exit 0
- [ ] **31.2 Publish**
  - [ ] Bump `0.5.0`, README (multi-source screenshots), tag, CI Linux + Windows, `SHA256SUMS.txt`
  - [ ] verify: `gh release view v0.5.0 --json assets`
