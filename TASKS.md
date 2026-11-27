# TASKS.md — Dalga Product Roadmap (v0.2.0 → v0.5.0)

Execution plan for turning the `ytm-architect` engineering suite into **Dalga**, an end-user native multi-source desktop music & media player (Rust + egui, Linux & Windows; starting with YouTube Music in v0.2.0–v0.4.0, expanding to Radio, Local Files, and Video in v0.5.0). The v0.1.0/v0.1.1 re-architecture plan (Phases 0–5, 43/43 tasks) is complete and archived in git history (checkpoint commit before this file).

Legend: `[ ]` open · `[x]` done. Every task ends with `verify:` checks that must pass before the box is ticked. Dates and gate outputs live only in `PROGRESS.md`.

---

## 0. Locked Decisions

| ID | Decision |
| :--- | :--- |
| B1 | **Brand:** visible name is **Dalga** (tagline `Dalga — Native Music & Media Player`). Applies to window title, app id display name, tray, icons, README. Binary names `ytm-gui` / `ytm` and crate names stay unchanged. |
| B2 | **Logo set:** Concept A approved (monochrome 3D continuous ribbon wave: 3 rising sound-wave peaks curling into a sea-wave crest; drafts saved in `assets/brand/drafts/concept-a-ribbon-{dark,light}.png`). Dark & light variants, lowercase geometric wordmark (`dalga`), and 1024px/16px-clean vector SVG/PNG exports. |
| B3 | **README:** centered logo via `<picture>` (auto light/dark on GitHub), badges (version, license, platform), real screenshots (Home, Search, Mini Player, light/dark side by side), feature table, download + SHA-256 verification, short "Why Dalga" section. |
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
| N1 | **Finalized brand name: `Dalga`.** Replaced provisional `YTMuse`; crates.io clean (404), GitHub check only found 0-star `dalga-music`. All brand strings still live in one constants module (Task 2.4) for clean maintainability. |
| K1 | **Multi-source player (v0.5.0):** YouTube Music, Radio, Local Files, Video are *sources*; YouTube Music is one of them. |
| K2 | **Radio:** bundled 551 Turkish stations (from `waldseelen/RadioStack` `stations.m3u`), user M3U/M3U8/XSPF import/export, optional Radio Browser directory search; favourites, categories, ICY Now Playing, auto-retry, offline badge. Local SQLite only, no server/Firebase. |
| K3 | **Video:** mpv in a separate borderless video window (YT Music video version, local video files); player bar, queue and MPRIS unchanged, no double playback. Embedded libmpv render is deferred to a later release after risk measurement. |
| K4 | **Local library:** watched folders, tags + embedded covers via `lofty` indexed into SQLite, unified search box, drag-and-drop open, "Open with" registration, local playlists with M3U8 export (never mixed with YT Music playlists). |
| K5 | **Source abstraction first:** after v0.4.0, `MediaSource { YouTube{video_id}, Radio{station_id,url}, LocalFile{path} }` + `PlayableItem` drive player, queue, session restore and MPRIS. `Track` stays YouTube-specific; saved sessions stay backward compatible via serde defaults. |
| K6 | **Dependencies not bundled:** mpv, yt-dlp, ffmpeg are detected by a first-run "System Check" screen (found/missing badges, per-platform one-line install command for pacman/apt/winget/scoop, Recheck). README documents the official Windows mpv download and SHA-256. |
| G1 | **Inherited gates (unchanged):** `cargo fmt --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, no `.rs` file > 400 lines, zero new `#[allow(...)]`, no emoji in UI strings. |

---

## Phase v0.2.0 — "Dalga": Brand, Theme, Chrome, Session

### Task 1 — Contract lock & docs
- [x] **1.1 Domain contracts**
  - [x] Add `ThemeMode`, `AccentPalette`, `UiLang`, `StatusMessage`, `StatusPriority` enums/structs with serde in `ytm-gui` (no parallel bool flags)
  - [x] Persist new fields in `GuiAppConfig` with `#[serde(default)]` so old `config.toml` still loads
  - [x] verify: unit test loads a v0.1.1 `config.toml` fixture and yields defaults
- [x] **1.2 Docs sync**
  - [x] Update `SPEC.md`, `ARCHITECTURE.md`, `GLOSSARY.md` for Dalga terms
  - [x] verify: `rg -n "ytm-architect" README.md SPEC.md` lists only intentional binary/crate references

### Task 2 — Brand & logo
- [x] **2.1 Logo generation & vectorization**
  - [x] Generate dark-variant and light-variant monochrome 3D draft concepts (`concept-a-ribbon-{dark,light}.png` approved by owner)
  - [x] Produce clean SVG vector + horizontal `dalga` wordmark + 1024px icon & 16px tray silhouette from Concept A
  - [x] verify: final vector/wordmark drafts approved and recorded in `PROGRESS.md`
- [x] **2.2 Asset packaging**
  - [x] Store sources under `assets/brand/`; export PNG sizes, `.ico` (Windows), `.desktop` + hicolor icons (Linux)
  - [x] Embed window icon via `ViewportBuilder::with_icon`
  - [x] verify: `file assets/brand/*.png` dimensions match; app launches with icon on Linux
- [x] **2.3 Rename visible strings**
  - [x] Window title, top-bar label, About, tray title → `Dalga`
  - [x] verify: `rg -n '"ytm-architect"' crates/ytm-gui/src` returns only app-id/config-path constants
- [x] **2.4 Central brand constants (N1)**
  - [x] `brand.rs` exposes `APP_NAME`, `TAGLINE`, `WINDOW_TITLE`, tray title; every visible brand string reads from it
  - [x] verify: unit test asserts no other module contains the literal `"Dalga"` (`rg -n 'Dalga' crates/ytm-gui/src` hits only `brand.rs` and tests)

### Task 3 — Theme engine
- [x] **3.1 Palettes**
  - [x] Replace constants in `theme.rs` with `Palette` structs for Ultra-Dark and Light surfaces
  - [x] Accent palettes: Gold, YTM Pink, Emerald
  - [x] verify: unit test asserts WCAG AA contrast (>= 4.5) for text on each surface
- [x] **3.2 System theme detection**
  - [x] Linux: XDG desktop portal `color-scheme` with `gsettings` fallback; Windows: registry `AppsUseLightTheme`
  - [x] Live-follow OS changes when mode is `System`
  - [x] verify: unit tests for parser functions (portal value 0/1/2, registry 0/1); manual toggle on Hyprland
- [x] **3.3 Dynamic album accent**
  - [x] Extract dominant colour off the UI thread (downscaled, quantized), clamp lightness for contrast
  - [x] Smooth transition; fall back to selected accent
  - [x] verify: unit test on fixture images returns stable colour; frame time unaffected (no work in `update`)
- [x] **3.4 Theme controls**
  - [x] Top-bar quick switch + Settings page (mode, accent, dynamic accent toggle)
  - [x] verify: restart restores selection; clippy + tests green

### Task 4 — Icon system (Lucide)
- [x] **4.1 SVG pipeline**
  - [x] Vendor required Lucide SVGs under `assets/icons/`; enable `egui_extras` `svg` feature; `icons.rs` exposes typed `Icon` enum
  - [x] Tint by theme colour, cache textures
  - [x] verify: unit test every `Icon` variant maps to an existing embedded file
- [x] **4.2 Replace glyphs**
  - [x] Replace every `✓ ✕ ♥ ⚠ ▶ ⏸ ⇄ ⋮⋮ ↻ ⇅ ⊘` and text mute labels across `views/*`, `overlays.rs`, `navigation.rs`, `play_queue.rs`, `interaction.rs`
  - [x] verify: `rg -n "[✓✕♥♡⚠▶◀⏸⏹⏮⏭⇄↻⇅⊘♪]" crates/ytm-gui/src --glob '!*_tests.rs'` returns nothing

### Task 5 — i18n (TR/EN)
- [x] **5.1 String catalog**
  - [x] `i18n` module with `Key` enum and static TR/EN tables; `tr!(Key)` lookup
  - [x] verify: test asserts every `Key` has non-empty TR and EN entries
- [x] **5.2 Migrate UI strings**
  - [x] Move top bar, sidebar, player bar, Settings, Auth, toasts to catalog
  - [x] verify: switching language updates UI without restart; `rg` finds no hard-coded English button labels in migrated views

### Task 6 — Application chrome
- [x] **6.1 Top bar redesign**
  - [x] Back/Forward, centered search box, theme/accent, Mini Player placeholder, profile/settings; remove `Backup All`, `Refresh Library`, shortcut strip (moved to menus/palette)
  - [x] verify: screenshot compare in light and dark; `Ctrl+K` palette still lists moved actions
- [x] **6.2 Studio Tools menu**
  - [x] Collapsible sidebar section containing Dedupe, Sort, Set Ops, Health, Genre Split, Snapshots, Sync
  - [x] verify: every former workspace tab reachable; navigation history tests pass
- [x] **6.3 Lyrics & Queue drawer**
  - [x] Separate player-bar buttons; right drawer with tabs; lyrics usable without opening queue
  - [x] verify: unit test for drawer state machine (`Closed | Queue | Lyrics`); manual: lyrics visible with queue closed
- [x] **6.4 Album-art Lightbox**
  - [x] Click cover → large modal image; click outside or `Esc` closes
  - [x] verify: unit test for open/close reducer; manual outside-click and `Esc`

### Task 7 — Status Ribbon
- [x] **7.1 Message bus**
  - [x] `StatusBus` with priority queue, sticky errors, TTL for results, history ring (50)
  - [x] Route existing `ToastState` producers and `WorkerEvent::StatusToast` into it
  - [x] verify: unit tests for priority ordering, sticky-until-dismiss, ring eviction
- [x] **7.2 Ticker widget**
  - [x] Right-to-left scroll with speed setting; click opens history popup; reduced-motion = static fade
  - [x] verify: unit test for scroll offset math with injected clock; manual visual check
- [x] **7.3 Greetings**
  - [x] Embedded TR/EN phrase lists by time of day; rate limit 1/hour; suppressed while playing and focused; Settings toggle
  - [x] verify: unit test with injected clock/RNG covers rate limit and suppression

### Task 8 — Instant search engine
- [x] **8.1 Core endpoints**
  - [x] `ytm-core`: `get_search_suggestions` (`music/get_search_suggestions`) with fixture-tested parser
  - [x] verify: fixture test in `ytm-core/tests`
- [x] **8.2 Search controller**
  - [x] Debounce 100 ms suggestions / 250 ms live results, request IDs to drop stale responses, LRU recent queries persisted
  - [x] verify: unit tests with fake client: stale response ignored, debounce coalesces keystrokes
- [x] **8.3 Dropdown UI**
  - [x] Top match + 3 songs + artist + album; keyboard navigation; `Enter` opens full results with filter tabs
  - [x] verify: manual keyboard walkthrough; measured p50 suggestion latency logged in `PROGRESS.md`

### Task 9 — Session refresh (S1)
- [x] **9.1 Browser launcher**
  - [x] Cross-platform `open_url` (xdg-open / `start`), configurable browser; builds `watch?v=` URL
  - [x] verify: unit test for URL builder; command construction tested without spawning
- [x] **9.2 Refresh flow**
  - [x] "Refresh Session" button + startup banner; wait ~6–8 s, run browser-cookie import, toast result via Status Ribbon
  - [x] Settings toggle for fully automatic refresh (default OFF)
  - [x] verify: unit tests for state machine (`Idle | Opening | Importing | Done | Failed`); manual end-to-end on Brave
- [x] **9.3 Keyring noise**
  - [x] Confirm `YTM_USE_SECRET_SERVICE` opt-in and Wayland no-vsync fixes remain covered by tests
  - [x] verify: existing tests `secret_service_is_opt_in...` and `vsync_disabled_on_linux_wayland_only` pass

### Task 10 — Offline-first cache (P1)
- [x] **10.1 Persisted caches**
  - [x] Lyrics + library summaries stored in SQLite alongside playlists; versioned schema migration
  - [x] verify: store integration test round-trips and migrates an old DB fixture
- [x] **10.2 Stale-while-revalidate**
  - [x] Paint from cache first; refresh in background; offline banner when network fails
  - [x] verify: test with failing client still yields cached library state

### Task 11 — Jump to YouTube (Y1)
- [x] **11.1 Link builder**
  - [x] Pure functions for YT Music URL (track/album/artist/playlist), YouTube search URL, `&t=` variant
  - [x] verify: table-driven unit tests with URL-encoding edge cases
- [x] **11.2 Wiring**
  - [x] Context menu, player bar, palette actions; `Ctrl+O`, `Ctrl+Shift+O`
  - [x] verify: manual open in default browser; shortcut test in `interaction.rs`

### Task 12 — README & release v0.2.0
- [x] **12.1 Screenshots**
  - [x] Capture Home, Search, Lightbox, light/dark pair into `assets/screenshots/`
  - [x] verify: files referenced by README exist (`rg -o "assets/[^)\" ]+" README.md | sort -u | xargs ls`)
- [x] **12.2 README rewrite (English)**
  - [x] Hero `<picture>` logo, badges, features table, install + SHA-256 verification, usage, "Why Dalga"
  - [x] verify: GitHub renders light/dark logos (check after push); links resolve
- [x] **12.3 Phase gate & release**
  - [x] Bump to `0.2.0`; run G1 gates; tag `v0.2.0`; CI builds Linux + Windows; publish with `SHA256SUMS.txt`; write hashes into README + `PROGRESS.md`
  - [x] verify: `gh run view` success for both jobs; `gh release view v0.2.0 --json assets`

---

## Phase v0.3.0 — Tray, Mini Player, Home, Shortcuts

### Task 13 — Window lifecycle & tray (M1)
- [x] **13.1 Tray abstraction**
  - [x] `TrayBackend` trait; Linux `ksni` implementation, Windows `tray-icon` implementation, no-op fallback when no tray host
  - [x] verify: trait tests with fake backend; build passes on both targets in CI
- [x] **13.2 Tray menu & icon**
  - [x] Theme-aware icon; menu: Show/Hide, Play/Pause, Next, Previous, Mini Player, Quit
  - [x] verify: manual on Waybar (Linux); Windows verified in CI-built binary by owner
- [x] **13.3 Close-to-tray**
  - [x] Intercept window close when tray active; playback continues; quit only from tray/menu; Settings toggle
  - [x] verify: unit test for close-request reducer (`Quit | HideToTray`)

### Task 14 — Mini Player
- [x] **14.1 Secondary viewport**
  - [x] Borderless, always-on-top, draggable; position/size persisted per monitor
  - [x] verify: unit test for position clamping to visible monitors; manual drag on Hyprland (floating rule documented)
- [x] **14.2 Compact mode**
  - [x] Cover, title/artist, transport, like, shuffle, repeat, progress
  - [x] verify: state-sync test: actions in Mini Player reach `PlayerController` and reflect back
- [x] **14.3 Expanded mode**
  - [x] Search field reusing Task 8 controller; top 3 results; click plays; collapse back
  - [x] verify: unit test that Mini Player and main search share one controller instance
- [x] **14.4 Spotify-style behaviors**
  - [x] Snap to screen edges, opacity-on-idle option, resize handles
  - [x] verify: unit tests for snap math

### Task 15 — Home shelves & Continue card
- [x] **15.1 Home parser**
  - [x] Parse personalised shelves (Listen Again, Mixes, New Releases) with fixtures
  - [x] verify: `ytm-core` fixture tests, recursive-search helpers only (no index chains)
- [x] **15.2 Shelf UI**
  - [x] Card grid with horizontal scroll, skeleton loading, "Continue" card from saved playback session
  - [x] verify: manual; cache-first paint verified offline

### Task 16 — Consistent context menu
- [x] **16.1 Menu model**
  - [x] One `ContextTarget` enum (Track, Album, Artist, Playlist) → one menu builder: queue next/last, radio, add to playlist, share link, go to artist, All of Artist (disabled until v0.4.0), Jump to YouTube
  - [x] verify: table test: each target yields the expected action set
- [x] **16.2 Adopt everywhere**
  - [x] Replace per-view menus in Inspector, Search, Browse, Queue, Home
  - [x] verify: `rg -n "context_menu" crates/ytm-gui/src/views` shows only the shared builder

### Task 17 — Keyboard-first
- [x] **17.1 Shortcut registry**
  - [x] Central table of shortcuts (media keys, `Space`, `Ctrl+K`, `/` focus search, arrows, `Ctrl+O`, mini-player toggle)
  - [x] verify: registry test detects duplicate bindings
- [x] **17.2 Shortcuts page**
  - [x] Settings page listing shortcuts generated from the registry
  - [x] verify: page content equals registry (test)

### Task 18 — Release v0.3.0
- [x] **18.1 Gates & docs**
  - [x] G1 gates; update README features + screenshots (Mini Player, tray); `PROGRESS.md`
  - [x] verify: all G1 commands exit 0
- [x] **18.2 Publish**
  - [x] Bump `0.3.0`, tag, CI Linux + Windows, `SHA256SUMS.txt` in README
  - [x] verify: `gh release view v0.3.0 --json assets`

---

## Phase v0.4.0 — Power Features & Accessibility

### Task 19 — All of Artist (P2)
- [x] **19.1 Harvest**
  - [x] Fetch artist page → albums, singles, appears-on (with continuation); collect all tracks
  - [x] verify: fixture tests for artist browse + continuation
- [x] **19.2 Plan**
  - [x] Pure `ArtistHarvestPlan` in `ytm-ops`: dedupe (ATV preferred over UGC), options (include features/compilations)
  - [x] verify: unit tests in `ytm-ops` for dedupe priority and option filters
- [x] **19.3 Apply**
  - [x] Preview dialog → new or existing playlist → `SyncQueue` → verified POST with snapshot
  - [x] verify: mocked-client test asserts snapshot precedes mutation and read-back runs

### Task 20 — Playlist Quick-Add
- [x] **20.1 Target list**
  - [x] Player-bar one-click add to chosen favourite playlist; picker in Settings
  - [x] verify: unit test stages `AddTracks` with `allow_duplicates = false`

### Task 21 — Audio power tools
- [x] **21.1 EQ presets**
  - [x] mpv `af=equalizer` chains: Flat, Bass Boost, Vocal, Custom (5/10-band)
  - [x] verify: unit tests for filter-string builder; manual audible check
- [x] **21.2 Speed**
  - [x] 0.75x–1.5x via mpv `speed`, pitch-corrected
  - [x] verify: IPC command test with fake runtime
- [x] **21.3 Sleep timer**
  - [x] Timer with injected clock; fade-out then pause; status via Ribbon
  - [x] verify: unit tests for timer state machine

### Task 22 — History & notifications
- [x] **22.1 Recently played**
  - [x] Persist play history in SQLite; quick-access list on Home
  - [x] verify: store test for insert/cap/ordering
- [x] **22.2 Desktop notifications**
  - [x] Optional OS notification on track change (Linux `notify-send` / Windows hook); off by default
  - [x] verify: disabled-by-default test performs zero notification calls (G-rule 3.6)

### Task 23 — Accessibility
- [x] **23.1 Scale & contrast**
  - [x] Font scale setting, high-contrast palette, visible focus ring
  - [x] verify: contrast unit tests reused from Task 3.1
- [x] **23.2 Screen reader**
  - [x] Enable AccessKit; label icon-only buttons through the `Icon` enum tooltips
  - [x] verify: test asserts every icon button has a non-empty label

### Task 24 — Discovery extras
- [x] **24.1 Similar & radio**
  - [x] "Similar songs" from watch-next; radio from any target
  - [x] verify: fixture parser tests
- [x] **24.2 Last.fm scrobble (opt-in)**
  - [x] Scrobble after 50% / 4 min with injected clock; credentials stored like auth; disabled = zero network
  - [x] verify: unit tests for scrobble threshold; disabled-path test makes no HTTP call

### Task 25 — Release v0.4.0
- [x] **25.1 Full verification**
  - [x] G1 gates; `CHANGELOG.md` + release workflow notes extraction; `>400`-line splits verified
  - [x] verify: all G1 commands exit 0
- [x] **25.2 Publish**
  - [x] Bump `0.4.0`, README + `PROGRESS.md` sync, tag, CI Linux + Windows, `SHA256SUMS.txt`
  - [x] verify: `gh release view v0.4.0 --json assets`

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

---

## Phase 6 (v0.6.0) — Dalga Lyrics v2: Multi-Provider Sync Engine (`better-lyrics` Inspired)

> Zero-CAPTCHA / zero-Turnstile native Rust pipeline. Inspired by `/tmp/better-lyrics` (`selection.ts`, `providerPin.ts`, `segmentMap.ts`, `translation.ts`, `lyricsDock/`), adapted strictly to Dalga's Rust + SQLite + `egui` architecture.

### Task 32 — Lyrics Contracts, Parsers & Compatibility
- [ ] **32.1 Rich domain contracts & SQLite `schema v5`**
  - [ ] Extend `LyricsPayload` (`ytm-core`) backward-compatibly (`Rich(RichLyrics)` with word/syllable `TimedWord` parts, `is_instrumental`, `source_provider`, `#[serde(default)]` fields) so existing `v3`/`v4` `lyrics_cache` JSON continues to deserialize
  - [ ] Add `lyrics_meta` table in `ytm-store` (`video_id PRIMARY KEY, offset_ms, pinned_provider, translation_lang, translated_json, updated_at`) via idempotent `v4 -> v5` migration
  - [ ] verify: round-trip + legacy `v3`/`v4` `lyrics_cache` fixture deserialization & SQLite migration test in `ytm-store/tests`
- [ ] **32.2 Pure Rust LRC / Enhanced LRC / TTML & YouTube Captions (`fmt=json3`) parsers**
  - [ ] Implement fixture-tested LRC (`[mm:ss.xx]`), Enhanced LRC (`<mm:ss.xx>` word timings), TTML (`<p begin end><span>`), and YouTube manual `fmt=json3` caption parsers in `ytm-core` (skipping `[auto-generated]` ASR captions)
  - [ ] verify: table-driven parser tests in `ytm-core/tests` covering all-caps normalization, music-note trimming, and malformed lines

### Task 33 — Multi-Provider Race, Quiet-Window Selection & Pinning
- [ ] **33.1 Zero-Turnstile provider clients**
  - [ ] Implement LRCLIB REST client (`lrclib.net/api/get` exact duration+artist+title -> `/api/search` fuzzy fallback using `segment`/duration bounds) and local `.lrc`/`.ttml` override loader alongside native YouTube Music & YouTube Captions
  - [ ] verify: mock HTTP transport tests for exact hit, fuzzy fallback, instrumental flag, and timeout isolation
- [ ] **33.2 `LyricsSelection` quiet-window arbiter & per-track pinning**
  - [ ] Port `selection.ts` quiet-window coordinator (`Pending | Rejected | Displayable`, upgrade-only on higher-ranked provider arrival, quiet coalescing window) + `providerPin.ts` per-`video_id` pinning & 1-click source cycler
  - [ ] verify: deterministic state-machine unit tests (faster low-rank provider waits `quietMs`; slow top-rank provider upgrades seamlessly; pinned provider prioritized)

### Task 34 — Timeline Sync, `segmentMap`, Offset & Translation
- [ ] **34.1 `segmentMap` ATV ↔ OMV retiming & instrumental intermissions**
  - [ ] Extract `segmentMap` timing deltas (when present) to shift ATV lyrics onto OMV playback (`retime_to_segment_map`) and inject instrumental break rows for gaps > `5000 ms` with remaining countdown
  - [ ] verify: unit tests in `ytm-core` asserting copy-on-shift immutability and gap-coalescing boundaries
- [ ] **34.2 Per-track `+/- ms` timing offset & on-demand Translation/Romanization**
  - [ ] Add persisted per-track offset (`-500ms` / `-100ms` / `+100ms` / `+500ms` / `Reset`) applied at render time without re-fetching
  - [ ] Add opt-in, button-triggered Translation & non-Latin Romanization (off by default, zero network calls until clicked, cached in `lyrics_meta`)
  - [ ] verify: unit tests for non-Latin script detection, offset math, and disabled-by-default zero-network guarantee

### Task 35 — Lyrics UI v2 & Mini Player Lyrics Strip
- [ ] **35.1 Interactive drawer (`queue_lyrics.rs`)**
  - [ ] Click-to-Seek on any timed line, smart auto-scroll that pauses on manual wheel/drag and surfaces a `"Resume Scroll"` button, word-level highlight sweep, source badge + cycler, offset toolbar, and translate/romanize toggle
  - [ ] verify: manual check + unit tests for auto-scroll pause/resume state reducer
- [ ] **35.2 Mini Player synced strip**
  - [ ] Optional 1–2 line current/next synced lyric preview inside `mini_player_view.rs`
  - [ ] verify: G1 gates green; `wc -l <= 400` across all modified files

---

## Phase 7b — Core, Thread, CPU, Cache & Memory Optimization

> Data-driven runtime efficiency pass targeting observed hot spots (`worker.rs`, `artwork.rs`, `views/*.rs`, `ytm-store`), bracketed by measurable before/after metrics in `PROGRESS.md`.

### Task 36 — Thread & Runtime Consolidation
- [ ] **36.1 Bounded shared runtime & bridge elimination**
  - [ ] Cap worker Tokio runtime (`worker_threads`, `max_blocking_threads`) instead of unbounded core-count/512 defaults; remove dedicated `std::sync::mpsc` -> `tokio_mpsc` forwarding thread by sending directly to `tokio_mpsc`
  - [ ] Unify `artwork.rs` and background helpers onto shared bounded pool, offload `image::load_from_memory` + `FilterType::Triangle` downscaling to `spawn_blocking`, and avoid spawning background threads inside headless unit tests
  - [ ] verify: thread-count check via `/proc/self/status` before/after + zero regression in unit/integration suite

### Task 37 — CPU & Zero-Clone Frame Loop
- [ ] **37.1 Eliminate per-frame heap clones**
  - [ ] Replace hot UI per-frame `.to_vec()` calls (`views/queue_lyrics.rs`, `views/home_cards.rs`, `views/browse.rs`, `views/sync.rs`) with borrowed slices or `Arc<[T]>` snapshots; defer payload clones until click/action frames
  - [ ] Gate `ctx.request_repaint_after` strictly to active timed-lyrics / ticker / audio animation needs so idle/paused frames consume near-zero CPU
  - [ ] verify: headless render loop micro-benchmark comparing frame allocations and idle CPU before/after logged in `PROGRESS.md`

### Task 38 — Memory & Cache Budgets
- [ ] **38.1 Bounded RAM LRU for `playlist_cache` & GPU textures**
  - [ ] Bound in-memory `playlist_cache` with LRU eviction backed cleanly by SQLite `playlist_cache` (re-hydrating transparently on select) and enforce explicit byte + count budget on `ArtworkCache` GPU textures
  - [ ] verify: store/cache unit tests confirming LRU eviction preserves SQLite persistence and bounds peak RAM across 50 synthetic playlists
- [ ] **38.2 SQLite prepared statement & batch optimization**
  - [ ] Adopt `prepare_cached` and transactional batching in `ytm-store` (`offline_cache.rs`, `playlist_cache.rs`, `enrichment`)
  - [ ] verify: store micro-benchmark measuring bulk cache write/read latency reduction + G1 gates (`fmt`, `check`, `clippy -D warnings`, `test --locked`, `<= 400` lines) green
