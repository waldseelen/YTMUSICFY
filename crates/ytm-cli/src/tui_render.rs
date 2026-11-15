use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs};

use crate::tui::{ActiveTab, TuiApp};

/// Draws the full 3-zone Ratatui dashboard layout for `TuiApp`.
pub fn draw_app(app: &TuiApp, frame: &mut Frame<'_>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(2),
        ])
        .split(frame.area());

    draw_tabs(app, frame, chunks[0]);
    match app.active_tab {
        ActiveTab::Playlists => draw_playlists_tab(app, frame, chunks[1]),
        ActiveTab::Dedupe => draw_dedupe_tab(app, frame, chunks[1]),
        ActiveTab::Sort => draw_sort_tab(app, frame, chunks[1]),
        ActiveTab::Health => draw_health_tab(app, frame, chunks[1]),
        ActiveTab::Genres => draw_genres_tab(app, frame, chunks[1]),
    }
    draw_footer(app, frame, chunks[2]);

    if let Some(prompt) = &app.confirm_execute {
        let area = centered_rect(60, 25, frame.area());
        frame.render_widget(Clear, area);
        let modal = Paragraph::new(prompt.as_str()).block(
            Block::default()
                .title("Confirm Execution")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow)),
        );
        frame.render_widget(modal, area);
    }
}

fn draw_tabs(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let titles: Vec<Line<'_>> = ActiveTab::ALL
        .iter()
        .map(|t| Line::from(Span::raw(t.title())))
        .collect();
    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .title("ytm-architect")
                .borders(Borders::ALL),
        )
        .select(app.active_tab.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_widget(tabs, area);
}

fn draw_playlists_tab(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let rows: Vec<Row<'_>> = app
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let marker = if i == app.selected_idx { ">" } else { " " };
            let tracks = pl
                .track_count
                .map_or_else(|| "?".to_string(), |c| c.to_string());
            let style = if i == app.selected_idx {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(marker),
                Cell::from(pl.id.as_str()),
                Cell::from(pl.title.as_str()),
                Cell::from(tracks),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(2),
            Constraint::Length(26),
            Constraint::Min(20),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(["", "Playlist ID", "Title", "Tracks"]).style(Style::default().fg(Color::Gray)),
    )
    .block(
        Block::default()
            .title(format!(
                "Library Playlists ({}) | Snapshots: {}",
                app.playlists.len(),
                app.snapshots.len()
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn draw_dedupe_tab(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let Some(plan) = &app.dedupe_plan else {
        let msg = "Select a playlist and run `ytm dedupe <playlist>` to inspect clusters.";
        let empty = Paragraph::new(msg).block(
            Block::default()
                .title("Dedupe Preview")
                .borders(Borders::ALL),
        );
        frame.render_widget(empty, area);
        return;
    };

    let mut rows = Vec::new();
    for cluster in &plan.clusters {
        for rem in &cluster.removals {
            rows.push(Row::new(vec![
                Cell::from(cluster.keeper.title.clone()),
                Cell::from(rem.title.clone()),
                Cell::from(rem.music_video_type.short_label()),
                Cell::from(format!("{:.2}", cluster.similarity)),
            ]));
        }
    }
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(35),
            Constraint::Percentage(35),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(Row::new(["Keep", "Remove", "Type", "Sim"]))
    .block(
        Block::default()
            .title(format!(
                "Dedupe '{}' ({} removals)",
                plan.playlist_title, plan.total_removals
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn draw_sort_tab(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let Some(plan) = &app.sort_plan else {
        let msg = "Run `ytm sort <playlist> --by artist:asc` to preview LIS minimal moves.";
        let empty =
            Paragraph::new(msg).block(Block::default().title("Sort Preview").borders(Borders::ALL));
        frame.render_widget(empty, area);
        return;
    };

    let rows: Vec<Row<'_>> = plan
        .target_order
        .iter()
        .enumerate()
        .take(100)
        .map(|(i, t)| {
            Row::new(vec![
                Cell::from((i + 1).to_string()),
                Cell::from((t.position + 1).to_string()),
                Cell::from(t.primary_artist_name().unwrap_or("-").to_string()),
                Cell::from(t.title.clone()),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Percentage(35),
            Constraint::Min(20),
        ],
    )
    .header(Row::new(["New", "Old", "Artist", "Title"]))
    .block(
        Block::default()
            .title(format!(
                "Sort '{}' | Anchored={} Moves={}",
                plan.playlist_title,
                plan.anchored_count,
                plan.moves.len()
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn draw_health_tab(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let Some(report) = &app.health_report else {
        let msg = "Run `ytm health <playlist> --find-official` to audit tracks.";
        let empty =
            Paragraph::new(msg).block(Block::default().title("Health Audit").borders(Borders::ALL));
        frame.render_widget(empty, area);
        return;
    };

    let rows: Vec<Row<'_>> = report
        .issues
        .iter()
        .map(|issue| {
            Row::new(vec![
                Cell::from(issue.track.video_id.clone()),
                Cell::from(issue.track.title.clone()),
                Cell::from(format!("{:?}", issue.kind)),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Min(20),
            Constraint::Length(22),
        ],
    )
    .header(Row::new(["VideoID", "Title", "Issue"]))
    .block(
        Block::default()
            .title(format!(
                "Health '{}' | Issues={} Upgrades={}",
                report.playlist_title,
                report.issues.len(),
                report.upgrade_candidates.len()
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn draw_genres_tab(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let Some(plan) = &app.genre_plan else {
        let msg = "Run `ytm split-genre <playlist>` to inspect taxonomy buckets.";
        let empty = Paragraph::new(msg).block(
            Block::default()
                .title("Genre Buckets")
                .borders(Borders::ALL),
        );
        frame.render_widget(empty, area);
        return;
    };

    let rows: Vec<Row<'_>> = plan
        .buckets
        .iter()
        .map(|b| {
            Row::new(vec![
                Cell::from(b.genre.clone()),
                Cell::from(b.target_playlist_title.clone()),
                Cell::from(b.tracks_to_add.len().to_string()),
                Cell::from(b.already_present_count.to_string()),
            ])
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(16),
            Constraint::Min(24),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(Row::new(["Genre", "Playlist", "To Add", "Existing"]))
    .block(
        Block::default()
            .title(format!(
                "Genre Split '{}' | Skipped={}",
                plan.source_playlist_title, plan.skipped_small_buckets
            ))
            .borders(Borders::ALL),
    );
    frame.render_widget(table, area);
}

fn draw_footer(app: &TuiApp, frame: &mut Frame<'_>, area: Rect) {
    let text = format!(
        "[Tab/Left/Right] Tabs | [j/k] Navigate | [q] Quit  --  {}",
        app.status_banner
    );
    let footer = Paragraph::new(text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}
