use ytm_ops::{
    DedupeConfig, KeeperStrategy, SetAlgebraOp, SortDirection, SortField, SortKey, SortSpec,
};

/// Form parameters for Dedupe view.
#[derive(Debug, Clone)]
pub struct DedupeForm {
    pub threshold: f64,
    pub duration_tol: u32,
    pub ignore_duration: bool,
    pub exact_only: bool,
    pub keep: KeeperStrategy,
}

impl Default for DedupeForm {
    fn default() -> Self {
        let d = DedupeConfig::default();
        Self {
            threshold: d.fuzzy_threshold,
            duration_tol: d.duration_tolerance_sec,
            ignore_duration: d.ignore_duration,
            exact_only: d.exact_only,
            keep: d.keep,
        }
    }
}

/// Form parameters for LIS minimal-move Sort view.
#[derive(Debug, Clone)]
pub struct SortForm {
    pub keys: Vec<SortKey>,
    pub artist_spread: bool,
    pub use_seed: bool,
    pub seed: u64,
}

impl Default for SortForm {
    fn default() -> Self {
        Self {
            keys: vec![
                SortKey {
                    field: SortField::Artist,
                    direction: SortDirection::Asc,
                },
                SortKey {
                    field: SortField::Title,
                    direction: SortDirection::Asc,
                },
            ],
            artist_spread: false,
            use_seed: true,
            seed: 42,
        }
    }
}

impl SortForm {
    #[must_use]
    pub fn to_spec(&self) -> SortSpec {
        let has_shuffle = self.keys.iter().any(|k| k.field == SortField::Shuffle);
        SortSpec {
            keys: self.keys.clone(),
            artist_spread: self.artist_spread || has_shuffle,
            seed: if has_shuffle && self.use_seed {
                Some(self.seed)
            } else {
                None
            },
        }
    }
}

/// Form parameters for Set Algebra view.
#[derive(Debug, Clone)]
pub struct SetOpsForm {
    pub op: SetAlgebraOp,
    pub playlist_a: String,
    pub playlist_b: String,
    pub fuzzy: bool,
    pub target_playlist: String,
}

impl Default for SetOpsForm {
    fn default() -> Self {
        Self {
            op: SetAlgebraOp::Diff,
            playlist_a: "LM".to_string(),
            playlist_b: String::new(),
            fuzzy: true,
            target_playlist: String::new(),
        }
    }
}

/// Form parameters for Health Scanner view.
#[derive(Debug, Clone)]
pub struct HealthForm {
    pub find_official: bool,
    pub prune_unavailable: bool,
    pub upgrade: bool,
}

impl Default for HealthForm {
    fn default() -> Self {
        Self {
            find_official: true,
            prune_unavailable: true,
            upgrade: true,
        }
    }
}

/// Form parameters for Genre Split view.
#[derive(Debug, Clone)]
pub struct GenreSplitForm {
    pub min_tracks: usize,
    pub custom_genres_toml: String,
}

impl Default for GenreSplitForm {
    fn default() -> Self {
        Self {
            min_tracks: 5,
            custom_genres_toml: String::new(),
        }
    }
}

/// Search bar mode in the top-left sidebar (`Lists` filters titles, `AllTracks` searches entire repertoire).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarSearchMode {
    #[default]
    Playlists,
    AllTracks,
}

/// Default action when dropping tracks onto another playlist (`Ctrl`=Copy / `Shift`=Move still override).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DragDropMode {
    #[default]
    Ask,
    Move,
    Copy,
}

impl DragDropMode {
    pub const ALL: [Self; 3] = [Self::Ask, Self::Move, Self::Copy];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Move => "Move",
            Self::Copy => "Copy",
        }
    }
}

/// UI state for the horizontal Repertoire File-Tree search & drag-drop workspace.
#[derive(Debug, Clone)]
pub struct RepertoireTreeState {
    pub collapsed_playlists: std::collections::HashSet<String>,
    pub selected_keys: std::collections::HashSet<String>,
    pub target_playlist_id: Option<String>,
    pub split_ratio: f32,
}

impl Default for RepertoireTreeState {
    fn default() -> Self {
        Self {
            collapsed_playlists: std::collections::HashSet::new(),
            selected_keys: std::collections::HashSet::new(),
            target_playlist_id: None,
            split_ratio: 0.52,
        }
    }
}
