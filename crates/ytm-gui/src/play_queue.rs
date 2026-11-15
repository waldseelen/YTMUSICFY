use ytm_core::Track;

/// Playback repeat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    #[must_use]
    pub const fn cycle(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }

    #[must_use]
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Off => "🔁 Off",
            Self::All => "🔁 All",
            Self::One => "🔂 1",
        }
    }
}

/// Ordered audio playback queue with shuffle and repeat state.
#[derive(Debug, Clone, Default)]
pub struct PlayQueue {
    tracks: Vec<Track>,
    original: Vec<Track>,
    current_index: Option<usize>,
    shuffle: bool,
    repeat: RepeatMode,
    liked_video_ids: std::collections::HashSet<String>,
}

impl PlayQueue {
    #[must_use]
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    #[must_use]
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    #[must_use]
    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.and_then(|idx| self.tracks.get(idx))
    }

    #[must_use]
    pub const fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    #[must_use]
    pub const fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    pub fn cycle_repeat(&mut self) -> RepeatMode {
        self.repeat = self.repeat.cycle();
        self.repeat
    }

    #[must_use]
    pub fn is_liked(&self, video_id: &str) -> bool {
        self.liked_video_ids.contains(video_id)
    }

    pub fn set_liked(&mut self, video_id: &str, liked: bool) {
        if liked {
            self.liked_video_ids.insert(video_id.to_string());
        } else {
            self.liked_video_ids.remove(video_id);
        }
    }

    pub fn seed_liked_from_tracks(&mut self, tracks: &[Track]) {
        for t in tracks {
            self.liked_video_ids.insert(t.video_id.clone());
        }
    }

    /// Replace queue with `tracks`, starting playback at `start_idx`.
    pub fn start_with(
        &mut self,
        tracks: Vec<Track>,
        start_idx: usize,
        seed: u64,
    ) -> Option<&Track> {
        self.original = tracks;
        if self.original.is_empty() {
            self.tracks.clear();
            self.current_index = None;
            return None;
        }
        let clamped = start_idx.min(self.original.len() - 1);
        if self.shuffle {
            self.apply_shuffle_keeping(clamped, seed);
        } else {
            self.tracks = self.original.clone();
            self.current_index = Some(clamped);
        }
        self.current_track()
    }

    /// Insert `track` immediately after the currently playing item (or start if empty).
    pub fn play_next(&mut self, track: Track) -> bool {
        if self.tracks.is_empty() {
            self.start_with(vec![track], 0, 1);
            return true;
        }
        let insert_at = self
            .current_index
            .map_or(0, |i| (i + 1).min(self.tracks.len()));
        self.tracks.insert(insert_at, track.clone());
        self.original.push(track);
        false
    }

    /// Append `track` to the end of the queue. Returns `true` if queue was previously empty.
    pub fn append(&mut self, track: Track) -> bool {
        if self.tracks.is_empty() {
            self.start_with(vec![track], 0, 1);
            return true;
        }
        self.tracks.push(track.clone());
        self.original.push(track);
        false
    }

    /// Replace upcoming tracks (after `current_index`) with `radio_tracks`, preserving active item.
    pub fn replace_upcoming(&mut self, radio_tracks: Vec<Track>) {
        let Some(cur_idx) = self.current_index else {
            let _ = self.start_with(radio_tracks, 0, 1);
            return;
        };
        let cur_vid = self
            .tracks
            .get(cur_idx)
            .map(|t| t.video_id.clone())
            .unwrap_or_default();
        self.tracks.truncate(cur_idx + 1);
        for t in radio_tracks {
            if t.video_id != cur_vid {
                self.tracks.push(t);
            }
        }
        self.original = self.tracks.clone();
    }

    /// Advance to the next track according to `repeat` mode.
    pub fn advance(&mut self) -> Option<&Track> {
        let cur = self.current_index?;
        if self.tracks.is_empty() {
            self.current_index = None;
            return None;
        }
        match self.repeat {
            RepeatMode::One => self.tracks.get(cur),
            RepeatMode::All => {
                let next = (cur + 1) % self.tracks.len();
                self.current_index = Some(next);
                self.tracks.get(next)
            }
            RepeatMode::Off => {
                if cur + 1 < self.tracks.len() {
                    self.current_index = Some(cur + 1);
                    self.tracks.get(cur + 1)
                } else {
                    None
                }
            }
        }
    }

    /// Move to previous track (wrapping when `RepeatMode::All` is active).
    pub fn previous(&mut self) -> Option<&Track> {
        let cur = self.current_index?;
        if self.tracks.is_empty() {
            return None;
        }
        let prev = if cur > 0 {
            cur - 1
        } else if self.repeat == RepeatMode::All {
            self.tracks.len() - 1
        } else {
            0
        };
        self.current_index = Some(prev);
        self.tracks.get(prev)
    }

    /// Jump directly to queue position `idx`.
    pub fn jump_to(&mut self, idx: usize) -> Option<&Track> {
        if idx < self.tracks.len() {
            self.current_index = Some(idx);
            self.tracks.get(idx)
        } else {
            None
        }
    }

    /// Move queue item at `from` to `to`, keeping `current_index` pointing at the active track.
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.tracks.len() || to >= self.tracks.len() || from == to {
            return;
        }
        let item = self.tracks.remove(from);
        self.tracks.insert(to, item);
        self.current_index = self.current_index.map(|cur| {
            if cur == from {
                to
            } else if from < cur && cur <= to {
                cur - 1
            } else if to <= cur && cur < from {
                cur + 1
            } else {
                cur
            }
        });
    }

    /// Remove item at `idx`, adjusting `current_index` cleanly.
    pub fn remove(&mut self, idx: usize) {
        if idx >= self.tracks.len() {
            return;
        }
        self.tracks.remove(idx);
        if self.tracks.is_empty() {
            self.current_index = None;
            return;
        }
        if let Some(cur) = self.current_index {
            if idx < cur {
                self.current_index = Some(cur - 1);
            } else if cur >= self.tracks.len() {
                self.current_index = Some(self.tracks.len() - 1);
            }
        }
    }

    /// Clear all queued tracks and stop selection.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.original.clear();
        self.current_index = None;
    }

    /// Toggle shuffle mode while preserving whichever track is currently active.
    pub fn toggle_shuffle(&mut self, seed: u64) {
        let cur_video = self.current_track().map(|t| t.video_id.clone());
        self.shuffle = !self.shuffle;
        if self.tracks.is_empty() {
            return;
        }
        if self.shuffle {
            self.original = self.tracks.clone();
            let anchor = self.current_index.unwrap_or(0).min(self.original.len() - 1);
            self.apply_shuffle_keeping(anchor, seed);
        } else if !self.original.is_empty() {
            self.tracks = self.original.clone();
            self.current_index = cur_video
                .and_then(|vid| self.tracks.iter().position(|t| t.video_id == vid))
                .or(Some(0));
        }
    }

    fn apply_shuffle_keeping(&mut self, anchor_orig_idx: usize, seed: u64) {
        let anchor = self.original[anchor_orig_idx].clone();
        let mut rest: Vec<Track> = self
            .original
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != anchor_orig_idx)
            .map(|(_, t)| t.clone())
            .collect();
        let mut state = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        for i in (1..rest.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let j = (state as usize) % (i + 1);
            rest.swap(i, j);
        }
        self.tracks = std::iter::once(anchor).chain(rest).collect();
        self.current_index = Some(0);
    }
}

#[cfg(test)]
mod tests {
    use ytm_core::{MusicVideoType, TrackAvailability};

    use super::*;

    fn track(id: &str) -> Track {
        Track {
            video_id: id.to_string(),
            set_video_id: None,
            position: 0,
            title: format!("Title {id}"),
            artists: Vec::new(),
            album: None,
            duration_sec: Some(180),
            release_year: None,
            music_video_type: MusicVideoType::AudioTrack,
            availability: TrackAvailability::Available,
            thumbnail_url: None,
        }
    }

    #[test]
    fn queue_supports_play_next_repeat_shuffle_and_move() {
        let mut q = PlayQueue::default();
        q.start_with(vec![track("a"), track("b"), track("c")], 0, 42);
        assert_eq!(q.current_track().map(|t| t.video_id.as_str()), Some("a"));

        assert!(!q.play_next(track("next1")));
        assert_eq!(
            q.tracks()
                .iter()
                .map(|t| t.video_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "next1", "b", "c"]
        );

        q.move_item(2, 0);
        assert_eq!(q.current_track().map(|t| t.video_id.as_str()), Some("a"));
        assert_eq!(q.current_index(), Some(1));

        q.cycle_repeat(); // All
        assert_eq!(q.repeat_mode(), RepeatMode::All);
        assert_eq!(q.jump_to(3).map(|t| t.video_id.as_str()), Some("c"));
        assert_eq!(q.advance().map(|t| t.video_id.as_str()), Some("b"));

        q.toggle_shuffle(99);
        assert_eq!(q.current_track().map(|t| t.video_id.as_str()), Some("b"));
        q.toggle_shuffle(99);
        assert_eq!(q.current_track().map(|t| t.video_id.as_str()), Some("b"));
    }
}
