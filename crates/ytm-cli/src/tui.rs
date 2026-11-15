use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::{Frame, Terminal};
use ytm_core::{Playlist, PlaylistSummary};
use ytm_enrich::GenreSplitPlan;
use ytm_ops::{DedupePlan, HealthReport, SortPlan};
use ytm_store::SnapshotMetadata;

use crate::tui_render::draw_app;

/// Active workspace tab inside the Ratatui interactive dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Playlists,
    Dedupe,
    Sort,
    Health,
    Genres,
}

impl ActiveTab {
    pub const ALL: [Self; 5] = [
        Self::Playlists,
        Self::Dedupe,
        Self::Sort,
        Self::Health,
        Self::Genres,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Playlists => "Playlists",
            Self::Dedupe => "Dedupe",
            Self::Sort => "Sort",
            Self::Health => "Health",
            Self::Genres => "Genres",
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Playlists => 0,
            Self::Dedupe => 1,
            Self::Sort => 2,
            Self::Health => 3,
            Self::Genres => 4,
        }
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    #[must_use]
    pub const fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Logical UI command emitted by `TuiApp::handle_key`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiAction {
    None,
    Quit,
    InspectSelected(usize),
}

/// Interactive dashboard state supporting both live terminal and headless `TestBackend` rendering.
#[derive(Debug, Clone)]
pub struct TuiApp {
    pub active_tab: ActiveTab,
    pub selected_idx: usize,
    pub playlists: Vec<PlaylistSummary>,
    pub active_playlist: Option<Playlist>,
    pub dedupe_plan: Option<DedupePlan>,
    pub sort_plan: Option<SortPlan>,
    pub health_report: Option<HealthReport>,
    pub genre_plan: Option<GenreSplitPlan>,
    pub snapshots: Vec<SnapshotMetadata>,
    pub status_banner: String,
    pub confirm_execute: Option<String>,
}

impl Default for TuiApp {
    fn default() -> Self {
        Self {
            active_tab: ActiveTab::Playlists,
            selected_idx: 0,
            playlists: Vec::new(),
            active_playlist: None,
            dedupe_plan: None,
            sort_plan: None,
            health_report: None,
            genre_plan: None,
            snapshots: Vec::new(),
            status_banner: "DRY-RUN preview mode (read-only)".to_string(),
            confirm_execute: None,
        }
    }
}

impl TuiApp {
    /// Creates an initialized `TuiApp` seeded with fetched library summaries and snapshot history.
    #[must_use]
    pub fn with_library(playlists: Vec<PlaylistSummary>, snapshots: Vec<SnapshotMetadata>) -> Self {
        Self {
            playlists,
            snapshots,
            ..Default::default()
        }
    }

    /// Processes a single crossterm key event and returns next action.
    pub fn handle_key(&mut self, key: KeyEvent) -> TuiAction {
        if key.kind != KeyEventKind::Press {
            return TuiAction::None;
        }

        if self.confirm_execute.is_some() {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q')
            ) {
                self.confirm_execute = None;
            }
            return TuiAction::None;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => TuiAction::Quit,
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                self.active_tab = self.active_tab.next();
                TuiAction::None
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                self.active_tab = self.active_tab.prev();
                TuiAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.playlists.is_empty() && self.selected_idx + 1 < self.playlists.len() {
                    self.selected_idx += 1;
                }
                TuiAction::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_idx = self.selected_idx.saturating_sub(1);
                TuiAction::None
            }
            KeyCode::Enter => {
                if self.active_tab == ActiveTab::Playlists && !self.playlists.is_empty() {
                    TuiAction::InspectSelected(self.selected_idx)
                } else {
                    TuiAction::None
                }
            }
            _ => TuiAction::None,
        }
    }

    /// Draws the dashboard into `frame`.
    pub fn draw(&self, frame: &mut Frame<'_>) {
        draw_app(self, frame);
    }

    /// Renders a headless frame into `TestBackend` for automated layout verification.
    #[must_use]
    pub fn render_to_test_buffer(&self, width: u16, height: u16) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test backend terminal");
        terminal
            .draw(|f| self.draw(f))
            .expect("draw tui to test buffer");
        terminal.backend().buffer().clone()
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
    }
}

/// Starts the interactive Crossterm + Ratatui TUI event loop with automatic cleanup on exit.
pub fn run_tui(mut app: TuiApp) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let _guard = TerminalGuard;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| app.draw(f))?;
        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            match app.handle_key(key) {
                TuiAction::Quit => break,
                TuiAction::InspectSelected(idx) => {
                    if let Some(pl) = app.playlists.get(idx) {
                        app.status_banner = format!("Selected: {} ({})", pl.title, pl.id);
                    }
                }
                TuiAction::None => {}
            }
        }
    }
    Ok(())
}
