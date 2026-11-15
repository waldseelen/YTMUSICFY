use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use ytm_core::{Playlist, TrackAvailability};

use crate::error::StoreError;
use crate::models::ExportFormat;
use crate::paths::write_atomic;

/// Serialize a playlist into `.json`, `.csv`, or `.m3u8` string representation.
pub fn export_playlist_to_string(
    playlist: &Playlist,
    format: ExportFormat,
) -> Result<String, StoreError> {
    match format {
        ExportFormat::Json => serde_json::to_string_pretty(playlist)
            .map_err(|e| StoreError::Serialization(e.to_string())),
        ExportFormat::Csv => export_csv(playlist),
        ExportFormat::M3u8 => Ok(export_m3u8(playlist)),
    }
}

/// Export a playlist atomically to `path` using the requested [`ExportFormat`].
pub fn export_playlist_to_path(
    playlist: &Playlist,
    path: &Path,
    format: ExportFormat,
) -> Result<(), StoreError> {
    let content = export_playlist_to_string(playlist, format)?;
    write_atomic(path, content.as_bytes())
}

/// Load a [`Playlist`] from an exported JSON file.
pub fn import_playlist_from_json(path: &Path) -> Result<Playlist, StoreError> {
    let raw = fs::read_to_string(path).map_err(|source| StoreError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str::<Playlist>(&raw).map_err(|e| StoreError::Serialization(e.to_string()))
}

fn export_csv(playlist: &Playlist) -> Result<String, StoreError> {
    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record([
        "position",
        "video_id",
        "set_video_id",
        "title",
        "artists",
        "album",
        "duration_sec",
        "type",
        "availability",
    ])
    .map_err(|e| StoreError::Serialization(e.to_string()))?;

    for track in &playlist.tracks {
        let artists = track
            .artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join("; ");
        let album = track.album.as_ref().map(|a| a.name.as_str()).unwrap_or("");
        let duration = track
            .duration_sec
            .map(|d| d.to_string())
            .unwrap_or_default();
        let avail = match &track.availability {
            TrackAvailability::Available => "Available".to_string(),
            TrackAvailability::Deleted => "Deleted".to_string(),
            TrackAvailability::Private => "Private".to_string(),
            TrackAvailability::RegionBlocked => "RegionBlocked".to_string(),
            TrackAvailability::Unplayable(reason) => format!("Unplayable:{reason}"),
        };

        wtr.write_record([
            &track.position.to_string(),
            &track.video_id,
            track.set_video_id.as_deref().unwrap_or(""),
            &track.title,
            &artists,
            album,
            &duration,
            track.music_video_type.short_label(),
            &avail,
        ])
        .map_err(|e| StoreError::Serialization(e.to_string()))?;
    }

    let bytes = wtr
        .into_inner()
        .map_err(|e| StoreError::Serialization(e.to_string()))?;
    String::from_utf8(bytes).map_err(|e| StoreError::Serialization(e.to_string()))
}

fn export_m3u8(playlist: &Playlist) -> String {
    let mut out = String::from("#EXTM3U\n");
    let _ = writeln!(out, "#PLAYLIST:{}", playlist.title.replace('\n', " "));
    for track in &playlist.tracks {
        if track.video_id.trim().is_empty() {
            continue;
        }
        let dur = track.duration_sec.map(i64::from).unwrap_or(-1);
        let artist = track.primary_artist_name().unwrap_or("Unknown Artist");
        let _ = writeln!(
            out,
            "#EXTINF:{dur},{} - {}",
            artist.replace('\n', " "),
            track.title.replace('\n', " ")
        );
        let _ = writeln!(out, "https://music.youtube.com/watch?v={}", track.video_id);
    }
    out
}
