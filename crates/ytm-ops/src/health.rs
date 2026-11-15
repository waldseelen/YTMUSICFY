use std::collections::HashMap;

use ytm_core::{MusicVideoType, Playlist, Track, TrackAvailability};

use crate::dedupe::{fuzzy_track_similarity, within_duration_tolerance};
use crate::models::{HealthIssue, HealthIssueKind, HealthReport, UpgradeCandidate};
use crate::normalize::normalize_track;

/// Audits playability, missing `setVideoId`, and official `ATV` audio upgrade candidates.
#[must_use]
pub fn scan_playlist_health(
    playlist: &Playlist,
    search_candidates_by_video_id: &HashMap<String, Vec<Track>>,
    fuzzy_threshold: f64,
) -> HealthReport {
    let mut issues = Vec::new();
    let mut upgrade_candidates = Vec::new();

    for track in &playlist.tracks {
        match &track.availability {
            TrackAvailability::Available => {}
            TrackAvailability::Deleted => issues.push(HealthIssue {
                track: track.clone(),
                kind: HealthIssueKind::Deleted,
            }),
            TrackAvailability::Private => issues.push(HealthIssue {
                track: track.clone(),
                kind: HealthIssueKind::Private,
            }),
            TrackAvailability::RegionBlocked => issues.push(HealthIssue {
                track: track.clone(),
                kind: HealthIssueKind::RegionBlocked,
            }),
            TrackAvailability::Unplayable(reason) => issues.push(HealthIssue {
                track: track.clone(),
                kind: HealthIssueKind::Unplayable(reason.clone()),
            }),
        }

        if track.set_video_id.is_none() {
            issues.push(HealthIssue {
                track: track.clone(),
                kind: HealthIssueKind::MissingSetVideoId,
            });
        }

        let upgradeable_type = matches!(
            track.music_video_type,
            MusicVideoType::UserGenerated | MusicVideoType::OfficialMusicVideo
        );
        if upgradeable_type
            && let Some(candidates) = search_candidates_by_video_id.get(&track.video_id)
        {
            let source_norm = normalize_track(track);
            let mut best: Option<(Track, f64)> = None;

            for cand in candidates {
                if cand.music_video_type != MusicVideoType::AudioTrack
                    || !cand.availability.is_available()
                    || cand.video_id == track.video_id
                {
                    continue;
                }
                if !within_duration_tolerance(track, cand, 15, false) {
                    continue;
                }
                let cand_norm = normalize_track(cand);
                if let Some(sim) = fuzzy_track_similarity(&source_norm, &cand_norm, fuzzy_threshold)
                    && best.as_ref().is_none_or(|(_, prev)| sim > *prev)
                {
                    best = Some((cand.clone(), sim));
                }
            }

            if let Some((official_atv, similarity)) = best {
                upgrade_candidates.push(UpgradeCandidate {
                    source_track: track.clone(),
                    official_atv,
                    similarity,
                });
            }
        }
    }

    HealthReport {
        playlist_id: playlist.id.clone(),
        playlist_title: playlist.title.clone(),
        total_tracks: playlist.tracks.len(),
        issues,
        upgrade_candidates,
    }
}
