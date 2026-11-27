use std::collections::HashSet;

use crate::browse_models::{BrowsePage, BrowsePageKind, BrowseSection};
use crate::error::CoreError;
use crate::models::Track;
use crate::traits::InnerTubeClientTrait;

/// Artist page shelf category relevant to an "All of Artist" harvest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArtistSectionKind {
    Songs,
    Albums,
    Singles,
    AppearsOn,
    Other,
}

/// Classify an artist shelf title into a harvest category across TR/EN wording.
#[must_use]
pub fn classify_section_title(title: &str) -> ArtistSectionKind {
    let norm = title.trim().to_lowercase();
    if norm.is_empty() {
        return ArtistSectionKind::Other;
    }
    if norm.contains("single") || norm.contains("ep'ler") || norm.contains("epler") {
        ArtistSectionKind::Singles
    } else if norm.contains("appears on")
        || norm.contains("yer aldigi")
        || norm.contains("yer aldığı")
        || norm.contains("bulundugu")
        || norm.contains("bulunduğu")
    {
        ArtistSectionKind::AppearsOn
    } else if norm.contains("album") || norm.contains("albüm") {
        ArtistSectionKind::Albums
    } else if norm.contains("song")
        || norm.contains("track")
        || norm.contains("sarki")
        || norm.contains("şarkı")
    {
        ArtistSectionKind::Songs
    } else {
        ArtistSectionKind::Other
    }
}

/// Release (album / single / appears-on) discovered on an artist page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistReleaseRef {
    pub title: String,
    pub browse_id: String,
    pub kind: ArtistSectionKind,
}

/// Options controlling which artist shelves and how many pages are harvested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistHarvestOptions {
    pub include_top_songs: bool,
    pub include_albums: bool,
    pub include_singles: bool,
    pub include_appears_on: bool,
    pub max_releases: usize,
    pub max_pages: usize,
}

impl Default for ArtistHarvestOptions {
    fn default() -> Self {
        Self {
            include_top_songs: true,
            include_albums: true,
            include_singles: true,
            include_appears_on: true,
            max_releases: 40,
            max_pages: 5,
        }
    }
}

impl ArtistHarvestOptions {
    /// Returns true when the shelf category is enabled for this harvest.
    #[must_use]
    pub fn includes(&self, kind: ArtistSectionKind) -> bool {
        match kind {
            ArtistSectionKind::Songs => self.include_top_songs,
            ArtistSectionKind::Albums => self.include_albums,
            ArtistSectionKind::Singles => self.include_singles,
            ArtistSectionKind::AppearsOn => self.include_appears_on,
            ArtistSectionKind::Other => false,
        }
    }
}

/// One harvested track plus the artist shelf it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistHarvestTrack {
    pub track: Track,
    pub source: ArtistSectionKind,
    pub release_title: String,
}

/// Complete artist harvest result feeding the `ytm-ops` planning stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtistHarvest {
    pub artist_id: String,
    pub artist_name: String,
    pub releases: Vec<ArtistReleaseRef>,
    pub tracks: Vec<ArtistHarvestTrack>,
    pub pages_fetched: usize,
}

fn section_release_kind(section: &BrowseSection) -> Option<ArtistSectionKind> {
    let kind = classify_section_title(&section.title);
    (kind != ArtistSectionKind::Other).then_some(kind)
}

fn push_release(
    releases: &mut Vec<ArtistReleaseRef>,
    seen: &mut HashSet<String>,
    title: &str,
    browse_id: &str,
    kind: ArtistSectionKind,
    max: usize,
) {
    if releases.len() >= max || browse_id.trim().is_empty() {
        return;
    }
    if seen.insert(browse_id.to_string()) {
        releases.push(ArtistReleaseRef {
            title: title.to_string(),
            browse_id: browse_id.to_string(),
            kind,
        });
    }
}

fn collect_section_releases(
    section: &BrowseSection,
    kind: ArtistSectionKind,
    releases: &mut Vec<ArtistReleaseRef>,
    seen: &mut HashSet<String>,
    max: usize,
) {
    for card in &section.cards {
        if let Some(bid) = card.browse_id.as_deref() {
            push_release(releases, seen, &card.title, bid, kind, max);
        }
    }
}

/// Walk an artist page (plus continuations and "See all" shelves) collecting releases.
async fn collect_releases<C: InnerTubeClientTrait + ?Sized>(
    client: &C,
    page: &BrowsePage,
    opts: &ArtistHarvestOptions,
    releases: &mut Vec<ArtistReleaseRef>,
    seen: &mut HashSet<String>,
    pages_fetched: &mut usize,
) -> Result<(), CoreError> {
    let mut sections = page.sections.clone();
    let mut token = page.continuation.clone();
    let mut followed = 0usize;
    while let Some(next) = token.take() {
        if followed >= opts.max_pages {
            break;
        }
        followed += 1;
        let extra = client.get_browse_continuation(&next).await?;
        *pages_fetched += 1;
        token = extra.continuation.clone();
        sections.extend(extra.sections);
    }

    for section in &sections {
        let Some(kind) = section_release_kind(section) else {
            continue;
        };
        if kind == ArtistSectionKind::Songs || !opts.includes(kind) {
            continue;
        }
        collect_section_releases(section, kind, releases, seen, opts.max_releases);
        let (Some(see_all_id), Some(params)) = (
            section.browse_id.as_deref(),
            section.browse_params.as_deref(),
        ) else {
            continue;
        };
        if releases.len() >= opts.max_releases {
            break;
        }
        let mut sub = client
            .get_browse_page_with_params(see_all_id, Some(params))
            .await?;
        *pages_fetched += 1;
        let mut sub_pages = 0usize;
        loop {
            for sub_section in &sub.sections {
                collect_section_releases(sub_section, kind, releases, seen, opts.max_releases);
            }
            let Some(next) = sub.continuation.clone() else {
                break;
            };
            if sub_pages >= opts.max_pages {
                break;
            }
            sub_pages += 1;
            sub = client.get_browse_continuation(&next).await?;
            *pages_fetched += 1;
        }
    }
    Ok(())
}

/// Harvest every enabled release shelf of an artist into a flat candidate track list.
///
/// Tracks are returned in shelf/release order without deduplication; `ytm-ops`
/// (`plan_artist_harvest`) owns the ATV-preference dedupe and option filters.
pub async fn harvest_artist<C: InnerTubeClientTrait + ?Sized>(
    client: &C,
    artist_id: &str,
    opts: &ArtistHarvestOptions,
) -> Result<ArtistHarvest, CoreError> {
    let mut pages_fetched = 1usize;
    let page = client.get_browse_page(artist_id).await?;
    let artist_name = if page.title.trim().is_empty() {
        artist_id.to_string()
    } else {
        page.title.clone()
    };

    let mut tracks: Vec<ArtistHarvestTrack> = Vec::new();
    if opts.include_top_songs {
        for section in &page.sections {
            if classify_section_title(&section.title) == ArtistSectionKind::Songs {
                tracks.extend(
                    section
                        .tracks
                        .iter()
                        .cloned()
                        .map(|track| ArtistHarvestTrack {
                            track,
                            source: ArtistSectionKind::Songs,
                            release_title: section.title.clone(),
                        }),
                );
            }
        }
    }

    let mut releases = Vec::new();
    let mut seen = HashSet::new();
    collect_releases(
        client,
        &page,
        opts,
        &mut releases,
        &mut seen,
        &mut pages_fetched,
    )
    .await?;

    for release in &releases {
        let mut album_page = client.get_browse_page(&release.browse_id).await?;
        pages_fetched += 1;
        let mut followed = 0usize;
        loop {
            if matches!(
                album_page.kind,
                BrowsePageKind::Album | BrowsePageKind::Playlist | BrowsePageKind::Unknown
            ) {
                tracks.extend(
                    album_page
                        .tracks
                        .iter()
                        .cloned()
                        .map(|track| ArtistHarvestTrack {
                            track,
                            source: release.kind,
                            release_title: release.title.clone(),
                        }),
                );
            }
            let Some(next) = album_page.continuation.clone() else {
                break;
            };
            if followed >= opts.max_pages {
                break;
            }
            followed += 1;
            album_page = client.get_browse_continuation(&next).await?;
            pages_fetched += 1;
        }
    }

    Ok(ArtistHarvest {
        artist_id: artist_id.to_string(),
        artist_name,
        releases,
        tracks,
        pages_fetched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_tr_and_en_shelf_titles() {
        assert_eq!(
            classify_section_title("En çok dinlenen şarkılar"),
            ArtistSectionKind::Songs
        );
        assert_eq!(
            classify_section_title("Albümler"),
            ArtistSectionKind::Albums
        );
        assert_eq!(
            classify_section_title("Single'lar ve EP'ler"),
            ArtistSectionKind::Singles
        );
        assert_eq!(
            classify_section_title("Yer aldığı albümler"),
            ArtistSectionKind::AppearsOn
        );
        assert_eq!(
            classify_section_title("Appears on"),
            ArtistSectionKind::AppearsOn
        );
        assert_eq!(
            classify_section_title("Featured playlists"),
            ArtistSectionKind::Other
        );
    }

    #[test]
    fn options_filter_disabled_shelves() {
        let opts = ArtistHarvestOptions {
            include_appears_on: false,
            include_singles: false,
            ..ArtistHarvestOptions::default()
        };
        assert!(opts.includes(ArtistSectionKind::Albums));
        assert!(opts.includes(ArtistSectionKind::Songs));
        assert!(!opts.includes(ArtistSectionKind::Singles));
        assert!(!opts.includes(ArtistSectionKind::AppearsOn));
        assert!(!opts.includes(ArtistSectionKind::Other));
    }
}
