use eframe::egui;
use ytm_core::Track;

use crate::state::{InspectColumn, InspectSortState};

pub(crate) fn sort_column_label(col: InspectColumn) -> &'static str {
    match col {
        InspectColumn::Position => "#",
        InspectColumn::Title => "Title",
        InspectColumn::Artist => "Artist",
        InspectColumn::Album => "Album",
        InspectColumn::Duration => "Dur(s)",
        InspectColumn::Type => "Type",
        InspectColumn::Status => "Status",
    }
}

pub fn sort_header(
    header: &mut egui_extras::TableRow<'_, '_>,
    title: &str,
    col: InspectColumn,
    sort_state: &mut InspectSortState,
) {
    header.col(|ui| {
        let active = sort_state.column == col;
        let size = egui::vec2(
            ui.available_width().max(20.0),
            ui.available_height().max(20.0),
        );
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        let bg = if active {
            egui::Color32::from_rgb(40, 62, 78)
        } else if resp.hovered() {
            egui::Color32::from_rgb(44, 48, 56)
        } else {
            egui::Color32::TRANSPARENT
        };
        if bg != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, 3.0, bg);
        }

        let arrow = if active {
            if sort_state.asc { " ▲" } else { " ▼" }
        } else {
            ""
        };
        let color = if active {
            egui::Color32::from_rgb(130, 210, 250)
        } else {
            ui.visuals().strong_text_color()
        };
        let label = format!("{title}{arrow}");
        ui.painter().text(
            egui::pos2(rect.left() + 4.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.5),
            color,
        );

        let hint = if active && sort_state.asc {
            "Sorted A-Z (click for Z-A)"
        } else if active {
            "Sorted Z-A (click for A-Z)"
        } else {
            "Click column cell to sort A-Z / Z-A"
        };
        if resp.on_hover_text(hint).clicked() {
            if active {
                sort_state.asc = !sort_state.asc;
            } else {
                sort_state.column = col;
                sort_state.asc = true;
            }
        }
    });
}

pub fn sort_tracks_slice(tracks: &mut [&Track], sort: InspectSortState) {
    tracks.sort_by(|a, b| {
        let ord = match sort.column {
            InspectColumn::Position => a.position.cmp(&b.position),
            InspectColumn::Title => a
                .title
                .to_ascii_lowercase()
                .cmp(&b.title.to_ascii_lowercase()),
            InspectColumn::Artist => a
                .primary_artist_name()
                .unwrap_or("")
                .to_ascii_lowercase()
                .cmp(&b.primary_artist_name().unwrap_or("").to_ascii_lowercase()),
            InspectColumn::Album => a
                .album
                .as_ref()
                .map_or("", |x| x.name.as_str())
                .to_ascii_lowercase()
                .cmp(
                    &b.album
                        .as_ref()
                        .map_or("", |x| x.name.as_str())
                        .to_ascii_lowercase(),
                ),
            InspectColumn::Duration => a.duration_sec.cmp(&b.duration_sec),
            InspectColumn::Type => a
                .music_video_type
                .short_label()
                .cmp(b.music_video_type.short_label()),
            InspectColumn::Status => a
                .availability
                .is_available()
                .cmp(&b.availability.is_available()),
        };
        let ord = ord.then_with(|| a.position.cmp(&b.position));
        if sort.asc { ord } else { ord.reverse() }
    });
}
