use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Last.fm scrobble configuration (opt-in only).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LastFmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub session_key: String,
}

/// Scrobble state machine with injected clock.
#[derive(Debug, Clone, Default)]
pub enum ScrobbleState {
    #[default]
    Disabled,
    Idle {
        last_scrobble: Option<Instant>,
    },
    Pending {
        track: ScrobbleTrack,
        started_at: Instant,
        threshold_reached: bool,
    },
    Submitting {
        track: ScrobbleTrack,
    },
    Submitted {
        track: ScrobbleTrack,
        submitted_at: Instant,
    },
    Failed {
        track: ScrobbleTrack,
        error: String,
        retry_at: Option<Instant>,
    },
}

impl ScrobbleState {
    /// Check if scrobbling is enabled and ready.
    #[must_use]
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Start tracking a new track for potential scrobble.
    pub fn start_track(&mut self, track: ScrobbleTrack, now: Instant) {
        if matches!(self, Self::Disabled) {
            return;
        }
        *self = Self::Pending {
            track,
            started_at: now,
            threshold_reached: false,
        };
    }

    /// Update the scrobble state with current playback position.
    /// Returns `Some(track)` when the scrobble threshold is reached and ready to submit.
    pub fn update_position(
        &mut self,
        position_sec: f64,
        duration_sec: f64,
        _now: Instant,
    ) -> Option<ScrobbleTrack> {
        let Self::Pending {
            track,
            started_at: _,
            threshold_reached,
        } = self
        else {
            return None;
        };

        // Scrobble after 50% of track or 4 minutes (240 seconds), whichever comes first
        let threshold = (duration_sec * 0.5).min(240.0);
        if !(*threshold_reached) && position_sec >= threshold {
            *threshold_reached = true;
            return Some(track.clone());
        }
        None
    }

    /// Mark track as submitting.
    pub fn start_submission(&mut self) -> Option<ScrobbleTrack> {
        if let Self::Pending { track, .. } = self {
            let track = track.clone();
            *self = Self::Submitting {
                track: track.clone(),
            };
            Some(track)
        } else {
            None
        }
    }

    /// Mark submission as successful.
    pub fn mark_submitted(&mut self, track: ScrobbleTrack, now: Instant) {
        *self = Self::Submitted {
            track,
            submitted_at: now,
        };
    }

    /// Mark submission as failed.
    pub fn mark_failed(&mut self, track: ScrobbleTrack, error: String, now: Instant) {
        // Retry after 30 seconds
        let retry_at = now + Duration::from_secs(30);
        *self = Self::Failed {
            track,
            error,
            retry_at: Some(retry_at),
        };
    }

    /// Check if we should retry a failed submission.
    pub fn check_retry(&mut self, now: Instant) -> Option<ScrobbleTrack> {
        if let Self::Failed {
            track,
            retry_at: Some(retry_at),
            ..
        } = self
            && now >= *retry_at
        {
            let track = track.clone();
            *self = Self::Pending {
                track: track.clone(),
                started_at: now,
                threshold_reached: true, // Already past threshold
            };
            return Some(track);
        }
        None
    }
}

/// Track information for scrobbling.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScrobbleTrack {
    pub artist: String,
    pub title: String,
    pub album: Option<String>,
    pub duration_sec: u32,
    pub timestamp: i64, // Unix timestamp when track started playing
}

impl ScrobbleTrack {
    /// Create a scrobble track from a playing track.
    #[must_use]
    pub fn from_track(track: &ytm_core::Track, _now: Instant) -> Self {
        let artist = track
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs() as i64;
        Self {
            artist,
            title: track.title.clone(),
            album: track.album.as_ref().map(|a| a.name.clone()),
            duration_sec: track.duration_sec.unwrap_or(0),
            timestamp,
        }
    }
}

/// Last.fm API client for scrobbling.
pub struct LastFmClient {
    api_key: String,
    api_secret: String,
    http: reqwest::Client,
}

impl LastFmClient {
    /// Create a new Last.fm client with API credentials.
    #[must_use]
    pub fn new(api_key: String, api_secret: String) -> Self {
        Self {
            api_key,
            api_secret,
            http: reqwest::Client::new(),
        }
    }

    /// Submit a scrobble to Last.fm (now playing or scrobble).
    pub async fn scrobble(
        &self,
        session_key: &str,
        track: &ScrobbleTrack,
        now_playing: bool,
    ) -> Result<(), String> {
        let method = if now_playing {
            "track.updateNowPlaying"
        } else {
            "track.scrobble"
        };
        let timestamp = track.timestamp.to_string();
        let mut params = vec![
            ("method".to_string(), method.to_string()),
            ("api_key".to_string(), self.api_key.clone()),
            ("sk".to_string(), session_key.to_string()),
            ("artist".to_string(), track.artist.clone()),
            ("track".to_string(), track.title.clone()),
            ("timestamp".to_string(), timestamp),
        ];

        if let Some(album) = &track.album {
            params.push(("album".to_string(), album.clone()));
        }
        if track.duration_sec > 0 {
            params.push(("duration".to_string(), track.duration_sec.to_string()));
        }

        // Add API signature
        let sig = self.sign_request(&params);
        params.push(("api_sig".to_string(), sig));
        params.push(("format".to_string(), "json".to_string()));

        let response = self
            .http
            .post("https://ws.audioscrobbler.com/2.0/")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if status.is_success() {
            // Check for Last.fm API errors in the response
            if text.contains("error") {
                return Err(format!("Last.fm API error: {text}"));
            }
            Ok(())
        } else {
            Err(format!("Last.fm HTTP {status}: {text}"))
        }
    }

    /// Generate API signature for Last.fm request.
    fn sign_request(&self, params: &[(String, String)]) -> String {
        use sha2::{Digest, Sha256};

        let mut sorted: Vec<_> = params
            .iter()
            .map(|(k, v)| (k.as_bytes(), v.as_bytes()))
            .collect();
        sorted.sort_by(|a, b| a.0.cmp(b.0));

        let mut data = String::new();
        for (k, v) in sorted {
            data.push_str(std::str::from_utf8(k).unwrap_or(""));
            data.push_str(std::str::from_utf8(v).unwrap_or(""));
        }
        data.push_str(&self.api_secret);

        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn scrobble_state_machine() {
        let mut state = ScrobbleState::Disabled;
        assert!(!state.is_active());

        let now = Instant::now();
        let sys_now = SystemTime::now();
        let track = ScrobbleTrack {
            artist: "Test Artist".to_string(),
            title: "Test Track".to_string(),
            album: None,
            duration_sec: 200,
            timestamp: sys_now
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_secs() as i64,
        };

        // Can't start when disabled
        state.start_track(track.clone(), now);
        assert!(matches!(state, ScrobbleState::Disabled));

        // Enable and start
        state = ScrobbleState::Idle {
            last_scrobble: None,
        };
        state.start_track(track.clone(), now);
        assert!(matches!(state, ScrobbleState::Pending { .. }));

        // Update position below threshold
        let result = state.update_position(50.0, 200.0, now + Duration::from_secs(60));
        assert!(result.is_none());

        // Update position above threshold (50% = 100s)
        let result = state.update_position(150.0, 200.0, now + Duration::from_secs(160));
        assert!(result.is_some());
        assert_eq!(result.unwrap().title, "Test Track");

        // Start submission
        let track = state.start_submission();
        assert!(track.is_some());
        assert!(matches!(state, ScrobbleState::Submitting { .. }));

        // Mark submitted
        state.mark_submitted(track.unwrap(), now + Duration::from_secs(200));
        assert!(matches!(state, ScrobbleState::Submitted { .. }));
    }

    #[test]
    fn scrobble_threshold_calculation() {
        let track = ScrobbleTrack {
            artist: "A".to_string(),
            title: "T".to_string(),
            album: None,
            duration_sec: 200,
            timestamp: 0,
        };

        let mut state = ScrobbleState::Idle {
            last_scrobble: None,
        };
        let now = Instant::now();
        state.start_track(track.clone(), now);

        // 4 min = 240s, 50% of 200s = 100s, so threshold is min(100, 240) = 100s
        assert!(state.update_position(99.0, 200.0, now).is_none());
        assert!(state.update_position(100.0, 200.0, now).is_some());

        // For longer track (600s), threshold is min(300, 240) = 240s (4 min)
        let long_track = ScrobbleTrack {
            duration_sec: 600,
            ..track
        };
        state.start_track(long_track, now);
        assert!(state.update_position(239.0, 600.0, now).is_none());
        assert!(state.update_position(240.0, 600.0, now).is_some());
    }
}
