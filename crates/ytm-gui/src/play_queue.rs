use serde::{Deserialize, Serialize};
use ytm_core::Track;

const MAX_RECENTLY_PLAYED: usize = 50;

/// Playback repeat mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::All => "All",
            Self::One => "1",
        }
    }

    #[must_use]
    pub const fn icon_kind(self) -> crate::icons::Icon {
        match self {
            Self::Off | Self::All => crate::icons::Icon::Repeat,
            Self::One => crate::icons::Icon::RepeatOne,
        }
    }
}

/// Serializable snapshot of `PlayQueue` and playback volume for persistence across restarts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayQueueSnapshot {
    pub tracks: Vec<Track>,
    pub current_index: Option<usize>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    #[serde(default)]
    pub recently_played: Vec<Track>,
    pub volume: f64,
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
    recently_played: Vec<Track>,
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

    /// Peeks the next track that would play on EOF without mutating queue position.
    #[must_use]
    pub fn peek_next(&self) -> Option<&Track> {
        let cur = self.current_index?;
        match self.repeat {
            RepeatMode::One => self.tracks.get(cur),
            RepeatMode::All if !self.tracks.is_empty() => {
                self.tracks.get((cur + 1) % self.tracks.len())
            }
            _ => self.tracks.get(cur + 1),
        }
    }

    #[must_use]
    pub const fn is_shuffle(&self) -> bool {
        self.shuffle
    }

    #[must_use]
    pub const fn repeat_mode(&self) -> RepeatMode {
        self.repeat
    }

    #[must_use]
    pub fn recently_played(&self) -> &[Track] {
        &self.recently_played
    }

    #[must_use]
    pub fn to_snapshot(&self, volume: f64) -> PlayQueueSnapshot {
        PlayQueueSnapshot {
            tracks: self.tracks.clone(),
            current_index: self.current_index,
            shuffle: self.shuffle,
            repeat: self.repeat,
            recently_played: self.recently_played.clone(),
            volume,
        }
    }

    pub fn restore_from_snapshot(&mut self, snap: &PlayQueueSnapshot) {
        self.tracks = snap.tracks.clone();
        self.original = snap.tracks.clone();
        self.shuffle = snap.shuffle;
        self.repeat = snap.repeat;
        self.recently_played = snap.recently_played.clone();
        self.current_index = snap
            .current_index
            .filter(|_| !self.tracks.is_empty())
            .map(|idx| idx.min(self.tracks.len() - 1));
    }

    fn record_current_recent(&mut self) {
        if let Some(track) = self.current_track().cloned() {
            self.recently_played
                .retain(|t| t.video_id != track.video_id);
            self.recently_played.insert(0, track);
            self.recently_played.truncate(MAX_RECENTLY_PLAYED);
        }
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
        self.liked_video_ids
            .extend(tracks.iter().map(|t| t.video_id.clone()));
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
        self.record_current_recent();
        self.current_track()
    }

    /// Insert `track` immediately after active item (`after_current = true`) or append to end.
    fn insert_track(&mut self, track: Track, after_current: bool) -> bool {
        if self.tracks.is_empty() {
            self.start_with(vec![track], 0, 1);
            return true;
        }
        if after_current {
            let idx = self
                .current_index
                .map_or(0, |i| (i + 1).min(self.tracks.len()));
            self.tracks.insert(idx, track.clone());
        } else {
            self.tracks.push(track.clone());
        }
        self.original.push(track);
        false
    }

    /// Insert `track` immediately after the currently playing item (or start if empty).
    pub fn play_next(&mut self, track: Track) -> bool {
        self.insert_track(track, true)
    }

    /// Append `track` to the end of the queue. Returns `true` if queue was previously empty.
    pub fn append(&mut self, track: Track) -> bool {
        self.insert_track(track, false)
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
        self.tracks
            .extend(radio_tracks.into_iter().filter(|t| t.video_id != cur_vid));
        self.original = self.tracks.clone();
    }

    /// Advance to the next track according to `repeat` mode.
    pub fn advance(&mut self) -> Option<&Track> {
        let cur = self.current_index?;
        if self.tracks.is_empty() {
            self.current_index = None;
            return None;
        }
        let next = match self.repeat {
            RepeatMode::One => cur,
            RepeatMode::All => (cur + 1) % self.tracks.len(),
            RepeatMode::Off if cur + 1 < self.tracks.len() => cur + 1,
            RepeatMode::Off => return None,
        };
        self.current_index = Some(next);
        self.record_current_recent();
        self.tracks.get(next)
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
        self.record_current_recent();
        self.tracks.get(prev)
    }

    /// Jump directly to queue position `idx`.
    pub fn jump_to(&mut self, idx: usize) -> Option<&Track> {
        if idx < self.tracks.len() {
            self.current_index = Some(idx);
            self.record_current_recent();
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
        self.current_index = self.current_index.map(|cur| match cur {
            c if c == from => to,
            c if from < c && c <= to => c - 1,
            c if to <= c && c < from => c + 1,
            c => c,
        });
    }

    /// Remove item at `idx`, adjusting `current_index` cleanly.
    pub fn remove(&mut self, idx: usize) {
        if idx >= self.tracks.len() {
            return;
        }
        self.tracks.remove(idx);
        self.current_index = if self.tracks.is_empty() {
            None
        } else {
            self.current_index.map(|cur| {
                if idx < cur {
                    cur - 1
                } else {
                    cur.min(self.tracks.len() - 1)
                }
            })
        };
    }

    /// Clear all queued tracks and stop selection.
    pub fn clear(&mut self) {
        self.tracks.clear();
        self.original.clear();
        self.current_index = None;
    }

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
                .and_then(|v| self.tracks.iter().position(|t| t.video_id == v))
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
            rest.swap(i, (state as usize) % (i + 1));
        }
        self.tracks = std::iter::once(anchor).chain(rest).collect();
        self.current_index = Some(0);
    }
}

#[cfg(test)]
#[path = "play_queue_tests.rs"]
mod tests;
