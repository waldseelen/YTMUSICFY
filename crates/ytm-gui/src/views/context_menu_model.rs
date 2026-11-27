use ytm_core::{ArtistRef, BrowseCard, BrowseCardKind, MusicVideoType, Track, TrackAvailability};

use crate::youtube_links::{
    youtube_search_url, ytm_album_url, ytm_artist_url, ytm_playlist_url, ytm_track_url,
};

/// Unified entity target for right-click context menus across all views.
#[derive(Debug, Clone)]
pub enum ContextTarget {
    Track {
        track: Track,
        queue_list: Vec<Track>,
        row_idx: usize,
    },
    Album {
        title: String,
        subtitle: String,
        browse_id: String,
    },
    Artist {
        name: String,
        browse_id: String,
    },
    Playlist {
        id: String,
        title: String,
        browse_id: Option<String>,
    },
}

impl ContextTarget {
    /// Convert a `BrowseCard` from Home, Browse, or Search into a `ContextTarget`.
    #[must_use]
    pub fn from_browse_card(card: &BrowseCard) -> Option<Self> {
        match card.kind {
            BrowseCardKind::Track => {
                let vid = card.video_id.clone()?;
                let track = Track {
                    video_id: vid,
                    set_video_id: None,
                    position: 0,
                    title: card.title.clone(),
                    artists: if card.subtitle.is_empty() {
                        Vec::new()
                    } else {
                        vec![ArtistRef {
                            name: card.subtitle.clone(),
                            browse_id: None,
                        }]
                    },
                    album: None,
                    duration_sec: None,
                    release_year: None,
                    music_video_type: MusicVideoType::AudioTrack,
                    availability: TrackAvailability::Available,
                    thumbnail_url: card.thumbnail_url.clone(),
                };
                Some(Self::Track {
                    queue_list: vec![track.clone()],
                    track,
                    row_idx: 0,
                })
            }
            BrowseCardKind::Album => Some(Self::Album {
                title: card.title.clone(),
                subtitle: card.subtitle.clone(),
                browse_id: card.browse_id.clone()?,
            }),
            BrowseCardKind::Artist => Some(Self::Artist {
                name: card.title.clone(),
                browse_id: card.browse_id.clone()?,
            }),
            BrowseCardKind::Playlist => {
                let bid = card.browse_id.clone()?;
                Some(Self::Playlist {
                    id: bid.clone(),
                    title: card.title.clone(),
                    browse_id: Some(bid),
                })
            }
        }
    }

    #[must_use]
    pub fn ytm_url(&self) -> String {
        match self {
            Self::Track { track, .. } => ytm_track_url(&track.video_id, None),
            Self::Album { browse_id, .. } => ytm_album_url(browse_id),
            Self::Artist { browse_id, .. } => ytm_artist_url(browse_id),
            Self::Playlist { id, browse_id, .. } => {
                ytm_playlist_url(browse_id.as_deref().unwrap_or(id))
            }
        }
    }

    #[must_use]
    pub fn yt_search_url(&self) -> String {
        let query = match self {
            Self::Track { track, .. } => format!(
                "{} {}",
                track.primary_artist_name().unwrap_or(""),
                track.title
            ),
            Self::Album {
                title, subtitle, ..
            } => format!("{title} {subtitle}"),
            Self::Artist { name, .. } => name.clone(),
            Self::Playlist { title, .. } => title.clone(),
        };
        youtube_search_url(&query)
    }

    #[must_use]
    pub fn artist_browse(&self) -> Option<(String, String)> {
        match self {
            Self::Track { track, .. } => track
                .artists
                .first()
                .and_then(|a| a.browse_id.clone().map(|bid| (a.name.clone(), bid))),
            Self::Artist { name, browse_id } => Some((name.clone(), browse_id.clone())),
            _ => None,
        }
    }

    #[must_use]
    pub fn album_browse(&self) -> Option<(String, String)> {
        match self {
            Self::Track { track, .. } => track
                .album
                .as_ref()
                .and_then(|a| a.browse_id.clone().map(|bid| (a.name.clone(), bid))),
            Self::Album {
                title, browse_id, ..
            } => Some((title.clone(), browse_id.clone())),
            _ => None,
        }
    }
}

/// Canonical action identifiers emitted by the shared context menu model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextActionId {
    PlayNow,
    PlayNext,
    AddQueue,
    StartRadio,
    ToggleLike,
    AddToPlaylist,
    OpenBrowse,
    GoToArtist,
    GoToAlbum,
    AllOfArtist,
    OpenInYtm,
    SearchOnYoutube,
    CopyLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextMenuItemSpec {
    pub id: ContextActionId,
    pub enabled: bool,
}

/// Pure specification of menu actions for any `ContextTarget`.
#[must_use]
pub fn build_menu_spec(
    target: &ContextTarget,
    has_editable_playlists: bool,
) -> Vec<ContextMenuItemSpec> {
    let mut items = Vec::new();
    match target {
        ContextTarget::Track { track, .. } => {
            items.push(ContextMenuItemSpec {
                id: ContextActionId::PlayNow,
                enabled: true,
            });
            items.push(ContextMenuItemSpec {
                id: ContextActionId::PlayNext,
                enabled: true,
            });
            items.push(ContextMenuItemSpec {
                id: ContextActionId::AddQueue,
                enabled: true,
            });
            items.push(ContextMenuItemSpec {
                id: ContextActionId::StartRadio,
                enabled: true,
            });
            items.push(ContextMenuItemSpec {
                id: ContextActionId::ToggleLike,
                enabled: true,
            });
            if has_editable_playlists {
                items.push(ContextMenuItemSpec {
                    id: ContextActionId::AddToPlaylist,
                    enabled: true,
                });
            }
            let has_artist_browse = track
                .artists
                .first()
                .and_then(|a| a.browse_id.as_ref())
                .is_some();
            if has_artist_browse {
                items.push(ContextMenuItemSpec {
                    id: ContextActionId::GoToArtist,
                    enabled: true,
                });
            }
            items.push(ContextMenuItemSpec {
                id: ContextActionId::AllOfArtist,
                enabled: has_artist_browse,
            });
            if track
                .album
                .as_ref()
                .and_then(|a| a.browse_id.as_ref())
                .is_some()
            {
                items.push(ContextMenuItemSpec {
                    id: ContextActionId::GoToAlbum,
                    enabled: true,
                });
            }
        }
        ContextTarget::Album { .. } => {
            items.push(ContextMenuItemSpec {
                id: ContextActionId::OpenBrowse,
                enabled: true,
            });
            items.push(ContextMenuItemSpec {
                id: ContextActionId::AllOfArtist,
                enabled: false,
            });
        }
        ContextTarget::Artist { .. } => {
            items.push(ContextMenuItemSpec {
                id: ContextActionId::OpenBrowse,
                enabled: true,
            });
            items.push(ContextMenuItemSpec {
                id: ContextActionId::AllOfArtist,
                enabled: true,
            });
        }
        ContextTarget::Playlist { browse_id, .. } => {
            if browse_id.is_some() {
                items.push(ContextMenuItemSpec {
                    id: ContextActionId::OpenBrowse,
                    enabled: true,
                });
            }
        }
    }

    items.push(ContextMenuItemSpec {
        id: ContextActionId::OpenInYtm,
        enabled: true,
    });
    items.push(ContextMenuItemSpec {
        id: ContextActionId::SearchOnYoutube,
        enabled: true,
    });
    items.push(ContextMenuItemSpec {
        id: ContextActionId::CopyLink,
        enabled: true,
    });
    items
}

#[cfg(test)]
mod tests {
    use ytm_core::AlbumRef;

    use super::*;

    fn sample_track() -> Track {
        Track {
            video_id: "vid123".into(),
            set_video_id: None,
            position: 0,
            title: "Midnight Harbor".into(),
            artists: vec![ArtistRef {
                name: "Nora Solis".into(),
                browse_id: Some("UC_NORA".into()),
            }],
            album: Some(AlbumRef {
                name: "Coastal Echoes".into(),
                browse_id: Some("MPREb_COASTAL".into()),
            }),
            duration_sec: Some(240),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    #[test]
    fn context_targets_yield_expected_action_sets() {
        let track = sample_track();
        let t_target = ContextTarget::Track {
            queue_list: vec![track.clone()],
            track,
            row_idx: 0,
        };
        let t_spec = build_menu_spec(&t_target, true);
        let t_ids: Vec<ContextActionId> = t_spec.iter().map(|s| s.id).collect();
        assert_eq!(
            t_ids,
            vec![
                ContextActionId::PlayNow,
                ContextActionId::PlayNext,
                ContextActionId::AddQueue,
                ContextActionId::StartRadio,
                ContextActionId::ToggleLike,
                ContextActionId::AddToPlaylist,
                ContextActionId::GoToArtist,
                ContextActionId::AllOfArtist,
                ContextActionId::GoToAlbum,
                ContextActionId::OpenInYtm,
                ContextActionId::SearchOnYoutube,
                ContextActionId::CopyLink,
            ]
        );
        assert!(
            t_spec
                .iter()
                .find(|s| s.id == ContextActionId::AllOfArtist)
                .unwrap()
                .enabled
        );

        let album_spec = build_menu_spec(
            &ContextTarget::Album {
                title: "Coastal Echoes".into(),
                subtitle: "Nora Solis".into(),
                browse_id: "MPREb_COASTAL".into(),
            },
            false,
        );
        assert_eq!(
            album_spec.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![
                ContextActionId::OpenBrowse,
                ContextActionId::AllOfArtist,
                ContextActionId::OpenInYtm,
                ContextActionId::SearchOnYoutube,
                ContextActionId::CopyLink,
            ]
        );

        let artist_spec = build_menu_spec(
            &ContextTarget::Artist {
                name: "Nora Solis".into(),
                browse_id: "UC_NORA".into(),
            },
            false,
        );
        assert_eq!(
            artist_spec.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![
                ContextActionId::OpenBrowse,
                ContextActionId::AllOfArtist,
                ContextActionId::OpenInYtm,
                ContextActionId::SearchOnYoutube,
                ContextActionId::CopyLink,
            ]
        );

        let pl_spec = build_menu_spec(
            &ContextTarget::Playlist {
                id: "VLPL1".into(),
                title: "Synth".into(),
                browse_id: Some("VLPL1".into()),
            },
            true,
        );
        assert_eq!(
            pl_spec.iter().map(|s| s.id).collect::<Vec<_>>(),
            vec![
                ContextActionId::OpenBrowse,
                ContextActionId::OpenInYtm,
                ContextActionId::SearchOnYoutube,
                ContextActionId::CopyLink,
            ]
        );
    }
}
