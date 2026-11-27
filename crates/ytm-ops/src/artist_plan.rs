use std::collections::HashSet;

use ytm_core::{ArtistHarvest, ArtistSectionKind, Track};

use crate::dedupe::keeper_score;
use crate::normalize::{NormalizedTrack, normalize_track};

/// Plan-level switches for an "All of Artist" harvest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtistHarvestConfig {
    /// Keep tracks where the artist is only a featured guest on an "Appears on" release.
    pub include_features: bool,
    /// Keep tracks the artist owns on an "Appears on" release (compilations, soundtracks).
    pub include_compilations: bool,
    /// Drop tracks already present in the destination playlist.
    pub skip_existing: bool,
}

impl Default for ArtistHarvestConfig {
    fn default() -> Self {
        Self {
            include_features: true,
            include_compilations: true,
            skip_existing: true,
        }
    }
}

/// Deterministic, order-preserving harvest plan ready for `SyncQueue` staging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistHarvestPlan {
    pub artist_id: String,
    pub artist_name: String,
    pub tracks: Vec<Track>,
    pub kept_from_songs: usize,
    pub kept_from_albums: usize,
    pub kept_from_singles: usize,
    pub kept_from_appears_on: usize,
    pub filtered_features: usize,
    pub filtered_compilations: usize,
    pub duplicates_removed: usize,
    pub skipped_existing: usize,
}

impl ArtistHarvestPlan {
    /// Number of tracks the plan would append to the destination playlist.
    #[must_use]
    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }
}

fn is_feature(track: &Track, artist_name: &str) -> bool {
    let primary = track.primary_artist_name().unwrap_or_default().trim();
    !artist_name.is_empty() && !primary.eq_ignore_ascii_case(artist_name)
}

fn duration_close(a: Option<u32>, b: Option<u32>) -> bool {
    match (a, b) {
        (Some(da), Some(db)) => da.abs_diff(db) <= 15,
        _ => true,
    }
}

/// Keep the highest-quality copy of each normalized title/artist group, preferring ATV over UGC.
fn push_deduped(
    kept: &mut Vec<Track>,
    sources: &mut Vec<ArtistSectionKind>,
    seen_video_ids: &mut HashSet<String>,
    seen_keys: &mut Vec<(String, Option<u32>, usize)>,
    track: Track,
    source: ArtistSectionKind,
) -> bool {
    if !track.video_id.is_empty() && !seen_video_ids.insert(track.video_id.clone()) {
        return false;
    }
    let normalized: NormalizedTrack = normalize_track(&track);
    let key = normalized.canonical_key();
    let existing = seen_keys
        .iter()
        .find(|(candidate, duration, _)| {
            *candidate == key && duration_close(*duration, track.duration_sec)
        })
        .map(|(_, _, index)| *index);
    match existing {
        Some(index) => {
            if keeper_score(&track) > keeper_score(&kept[index]) {
                kept[index] = track;
                sources[index] = source;
            }
            false
        }
        None => {
            let index = kept.len();
            kept.push(track);
            sources.push(source);
            seen_keys.push((key, kept[index].duration_sec, index));
            true
        }
    }
}

/// Filter a raw `ytm_core::ArtistHarvest` into a deduplicated, option-aware track list.
///
/// Dedupe priority is deterministic: within a normalized title/artist group (and a
/// ±15s duration window) the highest `keeper_score` survives, which ranks ATV above
/// UGC uploads. First-seen order is preserved.
#[must_use]
pub fn plan_artist_harvest(
    harvest: &ArtistHarvest,
    existing: &[Track],
    config: &ArtistHarvestConfig,
) -> ArtistHarvestPlan {
    let mut existing_ids: HashSet<&str> = HashSet::new();
    let mut existing_keys: HashSet<String> = HashSet::new();
    if config.skip_existing {
        for track in existing {
            if !track.video_id.is_empty() {
                existing_ids.insert(track.video_id.as_str());
            }
            existing_keys.insert(normalize_track(track).canonical_key());
        }
    }

    let mut plan = ArtistHarvestPlan {
        artist_id: harvest.artist_id.clone(),
        artist_name: harvest.artist_name.clone(),
        tracks: Vec::new(),
        kept_from_songs: 0,
        kept_from_albums: 0,
        kept_from_singles: 0,
        kept_from_appears_on: 0,
        filtered_features: 0,
        filtered_compilations: 0,
        duplicates_removed: 0,
        skipped_existing: 0,
    };
    let mut seen_video_ids: HashSet<String> = HashSet::new();
    let mut seen_keys: Vec<(String, Option<u32>, usize)> = Vec::new();
    let mut sources: Vec<ArtistSectionKind> = Vec::new();

    for entry in &harvest.tracks {
        let track = &entry.track;
        if entry.source == ArtistSectionKind::AppearsOn {
            if is_feature(track, &harvest.artist_name) {
                if !config.include_features {
                    plan.filtered_features += 1;
                    continue;
                }
            } else if !config.include_compilations {
                plan.filtered_compilations += 1;
                continue;
            }
        }
        if config.skip_existing
            && (existing_ids.contains(track.video_id.as_str())
                || existing_keys.contains(&normalize_track(track).canonical_key()))
        {
            plan.skipped_existing += 1;
            continue;
        }
        if !push_deduped(
            &mut plan.tracks,
            &mut sources,
            &mut seen_video_ids,
            &mut seen_keys,
            track.clone(),
            entry.source,
        ) {
            plan.duplicates_removed += 1;
        }
    }

    for source in sources {
        match source {
            ArtistSectionKind::Songs => plan.kept_from_songs += 1,
            ArtistSectionKind::Albums => plan.kept_from_albums += 1,
            ArtistSectionKind::Singles => plan.kept_from_singles += 1,
            ArtistSectionKind::AppearsOn => plan.kept_from_appears_on += 1,
            ArtistSectionKind::Other => {}
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use ytm_core::{
        ArtistHarvestTrack, ArtistRef, ArtistReleaseRef, MusicVideoType, TrackAvailability,
    };

    use super::*;

    fn track(video_id: &str, title: &str, artist: &str, kind: MusicVideoType) -> Track {
        Track {
            video_id: video_id.to_string(),
            set_video_id: None,
            position: 0,
            title: title.to_string(),
            artists: vec![ArtistRef {
                name: artist.to_string(),
                browse_id: None,
            }],
            album: None,
            duration_sec: Some(200),
            release_year: Some(2024),
            music_video_type: kind,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    fn harvest(entries: Vec<ArtistHarvestTrack>) -> ArtistHarvest {
        ArtistHarvest {
            artist_id: "UC_NORA".to_string(),
            artist_name: "Nora Solis".to_string(),
            releases: vec![ArtistReleaseRef {
                title: "Coastal Echoes".to_string(),
                browse_id: "MPREb_COASTAL".to_string(),
                kind: ArtistSectionKind::Albums,
            }],
            tracks: entries,
            pages_fetched: 1,
        }
    }

    fn entry(track: Track, source: ArtistSectionKind) -> ArtistHarvestTrack {
        ArtistHarvestTrack {
            track,
            source,
            release_title: "Release".to_string(),
        }
    }

    #[test]
    fn dedupe_prefers_atv_over_ugc_and_keeps_first_position() {
        let ugc = track(
            "ugc_1",
            "Midnight Harbor (Official Video)",
            "Nora Solis",
            MusicVideoType::UserGenerated,
        );
        let atv = track(
            "atv_1",
            "Midnight Harbor",
            "Nora Solis",
            MusicVideoType::AudioTrack,
        );
        let other = track(
            "atv_2",
            "Paper Tides",
            "Nora Solis",
            MusicVideoType::AudioTrack,
        );
        let plan = plan_artist_harvest(
            &harvest(vec![
                entry(ugc, ArtistSectionKind::Songs),
                entry(atv, ArtistSectionKind::Albums),
                entry(other, ArtistSectionKind::Albums),
            ]),
            &[],
            &ArtistHarvestConfig::default(),
        );

        let ids: Vec<&str> = plan.tracks.iter().map(|t| t.video_id.as_str()).collect();
        assert_eq!(ids, vec!["atv_1", "atv_2"]);
        assert_eq!(plan.duplicates_removed, 1);
        assert_eq!(plan.kept_from_songs, 0);
        assert_eq!(plan.kept_from_albums, 2);
    }

    #[test]
    fn option_filters_split_features_from_compilations() {
        let feature = track(
            "feat_1",
            "Late Night",
            "Other Artist",
            MusicVideoType::AudioTrack,
        );
        let compilation = track(
            "comp_1",
            "Coastal Echoes (Live)",
            "Nora Solis",
            MusicVideoType::AudioTrack,
        );
        let data = harvest(vec![
            entry(feature, ArtistSectionKind::AppearsOn),
            entry(compilation, ArtistSectionKind::AppearsOn),
        ]);

        let no_features = plan_artist_harvest(
            &data,
            &[],
            &ArtistHarvestConfig {
                include_features: false,
                ..ArtistHarvestConfig::default()
            },
        );
        assert_eq!(no_features.tracks.len(), 1);
        assert_eq!(no_features.tracks[0].video_id, "comp_1");
        assert_eq!(no_features.filtered_features, 1);
        assert_eq!(no_features.filtered_compilations, 0);

        let no_compilations = plan_artist_harvest(
            &data,
            &[],
            &ArtistHarvestConfig {
                include_compilations: false,
                ..ArtistHarvestConfig::default()
            },
        );
        assert_eq!(no_compilations.tracks.len(), 1);
        assert_eq!(no_compilations.tracks[0].video_id, "feat_1");
        assert_eq!(no_compilations.filtered_compilations, 1);
        assert_eq!(no_compilations.filtered_features, 0);
    }

    #[test]
    fn skip_existing_drops_known_tracks() {
        let a = track(
            "atv_1",
            "Midnight Harbor",
            "Nora Solis",
            MusicVideoType::AudioTrack,
        );
        let b = track(
            "atv_2",
            "Paper Tides",
            "Nora Solis",
            MusicVideoType::AudioTrack,
        );
        let plan = plan_artist_harvest(
            &harvest(vec![
                entry(a, ArtistSectionKind::Albums),
                entry(b, ArtistSectionKind::Albums),
            ]),
            &[track(
                "atv_1",
                "Midnight Harbor",
                "Nora Solis",
                MusicVideoType::AudioTrack,
            )],
            &ArtistHarvestConfig::default(),
        );
        assert_eq!(plan.tracks.len(), 1);
        assert_eq!(plan.tracks[0].video_id, "atv_2");
        assert_eq!(plan.skipped_existing, 1);
    }
}
