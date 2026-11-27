use std::time::{Duration, Instant};

/// Sleep timer state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SleepTimerState {
    #[default]
    Disabled,
    Armed {
        remaining: Duration,
        started_at: Instant,
    },
    Fading {
        remaining: Duration,
        fade_duration: Duration,
        started_at: Instant,
    },
    Paused,
}

impl SleepTimerState {
    /// Create a new armed sleep timer with the given duration.
    #[must_use]
    pub fn new(duration: Duration) -> Self {
        Self::Armed {
            remaining: duration,
            started_at: Instant::now(),
        }
    }

    /// Update the timer with the current time, returning the new state.
    /// Returns `Some(Self)` if state changed, `None` if no change.
    #[must_use]
    pub fn tick(&mut self, now: Instant) -> Option<Self> {
        match *self {
            Self::Armed {
                remaining,
                started_at,
            } => {
                let elapsed = now.saturating_duration_since(started_at);
                if elapsed >= remaining {
                    *self = Self::Fading {
                        remaining: Duration::from_secs(15),
                        fade_duration: Duration::from_secs(15),
                        started_at: now,
                    };
                    Some(*self)
                } else {
                    let new_remaining = remaining - elapsed;
                    *self = Self::Armed {
                        remaining: new_remaining,
                        started_at: now,
                    };
                    Some(*self)
                }
            }
            Self::Fading {
                remaining: _,
                fade_duration,
                started_at,
            } => {
                let elapsed = now.saturating_duration_since(started_at);
                if elapsed >= fade_duration {
                    *self = Self::Paused;
                    Some(*self)
                } else {
                    let new_remaining = fade_duration - elapsed;
                    *self = Self::Fading {
                        remaining: new_remaining,
                        fade_duration,
                        started_at: now,
                    };
                    Some(*self)
                }
            }
            Self::Disabled | Self::Paused => None,
        }
    }

    /// Cancel the timer.
    #[must_use]
    pub fn cancel(&mut self) -> Self {
        let prev = *self;
        *self = Self::Disabled;
        prev
    }

    /// Pause the timer without resetting it.
    #[must_use]
    pub fn pause(&mut self) -> Self {
        let prev = *self;
        *self = Self::Paused;
        prev
    }

    /// Resume from paused.
    #[must_use]
    pub fn resume(&mut self) -> Self {
        let prev = *self;
        *self = Self::new(Duration::from_secs(0));
        prev
    }

    /// Get the remaining time as a human-readable string.
    #[must_use]
    pub fn remaining_str(&self) -> Option<String> {
        match *self {
            Self::Armed { remaining, .. } | Self::Fading { remaining, .. } => {
                let secs = remaining.as_secs();
                if secs < 60 {
                    Some(format!("{secs}s"))
                } else if secs < 3600 {
                    Some(format!("{}m {}s", secs / 60, secs % 60))
                } else {
                    Some(format!("{}h {}m", secs / 3600, (secs % 3600) / 60))
                }
            }
            Self::Disabled | Self::Paused => None,
        }
    }

    /// Check if the timer is active (armed or fading).
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(*self, Self::Armed { .. } | Self::Fading { .. })
    }
}

/// Built-in EQ presets for mpv `af=equalizer` filter chains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EqPreset {
    #[default]
    Flat,
    BassBoost,
    Vocal,
    Custom5Band,
    Custom10Band,
}

impl EqPreset {
    /// Get the mpv `af` filter string for this preset.
    #[must_use]
    pub fn filter_string(self) -> &'static str {
        match self {
            Self::Flat => "",
            Self::BassBoost => {
                "equalizer=f=60:width_type=h:width=50:g=6,equalizer=f=120:width_type=h:width=50:g=4"
            }
            Self::Vocal => {
                "equalizer=f=2000:width_type=h:width=1000:g=3,equalizer=f=4000:width_type=h:width=1000:g=2"
            }
            Self::Custom5Band => {
                "equalizer=f=60:width_type=h:width=50:g=0,equalizer=f=230:width_type=h:width=50:g=0,equalizer=f=910:width_type=h:width=50:g=0,equalizer=f=3600:width_type=h:width=50:g=0,equalizer=f=14000:width_type=h:width=50:g=0"
            }
            Self::Custom10Band => {
                "equalizer=f=31:width_type=h:width=50:g=0,equalizer=f=62:width_type=h:width=50:g=0,equalizer=f=125:width_type=h:width=50:g=0,equalizer=f=250:width_type=h:width=50:g=0,equalizer=f=500:width_type=h:width=50:g=0,equalizer=f=1000:width_type=h:width=50:g=0,equalizer=f=2000:width_type=h:width=50:g=0,equalizer=f=4000:width_type=h:width=50:g=0,equalizer=f=8000:width_type=h:width=50:g=0,equalizer=f=16000:width_type=h:width=50:g=0"
            }
        }
    }

    /// Human-readable label for UI.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Flat => "Flat",
            Self::BassBoost => "Bass Boost",
            Self::Vocal => "Vocal",
            Self::Custom5Band => "Custom (5-band)",
            Self::Custom10Band => "Custom (10-band)",
        }
    }

    /// All presets for UI iteration.
    pub const ALL: [Self; 5] = [
        Self::Flat,
        Self::BassBoost,
        Self::Vocal,
        Self::Custom5Band,
        Self::Custom10Band,
    ];
}

/// Playback speed range (0.75x to 1.5x, pitch-corrected via mpv `speed` + `scaletempo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaybackSpeed(pub u8); // stored as integer 75-150 representing 0.75x-1.50x

impl PlaybackSpeed {
    pub const MIN: f64 = 0.75;
    pub const MAX: f64 = 1.50;
    pub const STEP: f64 = 0.05;

    #[must_use]
    pub fn new(value: f64) -> Self {
        let clamped = value.clamp(Self::MIN, Self::MAX);
        let int_val = (clamped * 100.0).round() as u8;
        Self(int_val)
    }

    #[must_use]
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / 100.0
    }

    #[must_use]
    pub fn label(self) -> String {
        format!("{:.2}x", self.as_f64())
    }

    #[must_use]
    pub fn is_normal(self) -> bool {
        self.0 == 100
    }
}

impl Default for PlaybackSpeed {
    fn default() -> Self {
        Self(100)
    }
}

/// Combined audio effects state managed by the player.
#[derive(Debug, Clone, Default)]
pub struct AudioEffects {
    pub eq_preset: EqPreset,
    pub speed: PlaybackSpeed,
    pub sleep_timer: SleepTimerState,
}

impl AudioEffects {
    /// Build the full mpv filter chain from current EQ preset.
    #[must_use]
    pub fn build_filter_chain(&self) -> String {
        let mut parts = Vec::new();
        let eq = self.eq_preset.filter_string();
        if !eq.is_empty() {
            parts.push(eq.to_string());
        }
        if !self.speed.is_normal() {
            parts.push(format!("scaletempo=speed={:.2}", self.speed.as_f64()));
        }
        parts.join(",")
    }

    /// Returns true if any non-default effect is active.
    #[must_use]
    pub fn has_active_effects(&self) -> bool {
        !matches!(self.eq_preset, EqPreset::Flat)
            || !self.speed.is_normal()
            || self.sleep_timer.is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn sleep_timer_arms_and_counts_down() {
        let mut timer = SleepTimerState::new(Duration::from_millis(50));
        let now = Instant::now();
        assert!(timer.tick(now).is_some());
        sleep(Duration::from_millis(60));
        let now2 = Instant::now();
        assert!(timer.tick(now2).is_some());
        assert!(matches!(timer, SleepTimerState::Fading { .. }));
    }

    #[test]
    fn sleep_timer_cancel() {
        let mut timer = SleepTimerState::new(Duration::from_secs(60));
        let prev = timer.cancel();
        assert!(matches!(prev, SleepTimerState::Armed { .. }));
        assert!(matches!(timer, SleepTimerState::Disabled));
    }

    #[test]
    fn eq_preset_filter_strings() {
        assert_eq!(EqPreset::Flat.filter_string(), "");
        assert!(EqPreset::BassBoost.filter_string().contains("equalizer"));
        assert!(EqPreset::Vocal.filter_string().contains("equalizer"));
        assert!(EqPreset::Custom5Band.filter_string().contains("equalizer"));
        assert!(EqPreset::Custom10Band.filter_string().contains("equalizer"));
    }

    #[test]
    fn playback_speed_clamping() {
        assert_eq!(PlaybackSpeed::new(0.5).as_f64(), 0.75);
        assert_eq!(PlaybackSpeed::new(2.0).as_f64(), 1.50);
        assert_eq!(PlaybackSpeed::new(1.0).as_f64(), 1.00);
        assert_eq!(PlaybackSpeed::new(1.25).as_f64(), 1.25);
    }

    #[test]
    fn audio_effects_filter_chain() {
        let fx = AudioEffects {
            eq_preset: EqPreset::BassBoost,
            speed: PlaybackSpeed::new(1.25),
            sleep_timer: SleepTimerState::Disabled,
        };
        let chain = fx.build_filter_chain();
        assert!(chain.contains("equalizer"));
        assert!(chain.contains("scaletempo"));
        assert!(chain.contains("speed=1.25"));

        let flat = AudioEffects::default();
        assert_eq!(flat.build_filter_chain(), "");
    }
}
