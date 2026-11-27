use crate::i18n::ResolvedLang;

const HOUR_MS: u64 = 3_600_000;

/// Coarse time-of-day bucket for embedded greetings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl TimeOfDay {
    #[must_use]
    pub const fn from_hour(hour: u32) -> Self {
        match hour % 24 {
            5..=11 => Self::Morning,
            12..=17 => Self::Afternoon,
            18..=22 => Self::Evening,
            _ => Self::Night,
        }
    }
}

const EN_MORNING: &[&str] = &[
    "Good morning — ready for the first track.",
    "Fresh morning queue waiting.",
    "Morning session ready.",
    "Start the day with something crisp.",
];
const EN_AFTERNOON: &[&str] = &[
    "Good afternoon — library synced.",
    "Afternoon focus mode ready.",
    "Midday rotation at your fingertips.",
    "Keep the afternoon flowing.",
];
const EN_EVENING: &[&str] = &[
    "Good evening — wind down with Dalga.",
    "Evening listening session ready.",
    "Time for an evening mix.",
    "Dusk sets in — press Play.",
];
const EN_NIGHT: &[&str] = &[
    "Late-night listening ready.",
    "Quiet hours, crisp audio.",
    "Midnight rotation standing by.",
    "Night owl session ready.",
];

const TR_MORNING: &[&str] = &[
    "Gunaydin — ilk parca icin hazir.",
    "Sabah dinleme listesi seni bekliyor.",
    "Gune temiz bir baslangic yap.",
    "Sabah oturumu hazir.",
];
const TR_AFTERNOON: &[&str] = &[
    "Iyi gunler — kitaplik hazir.",
    "Ogle sonrasi odaklanma modu hazir.",
    "Gun ortasi akisi parmaklarinin ucunda.",
    "Muzik akmaya devam etsin.",
];
const TR_EVENING: &[&str] = &[
    "Iyi aksamlar — Dalga ile gunu yumusat.",
    "Aksam dinleme oturumu hazir.",
    "Aksam karisimi icin guzel bir an.",
    "Gun batiminda Oynat'a bas.",
];
const TR_NIGHT: &[&str] = &[
    "Gece dinlemesi icin hazir.",
    "Sessiz saatler, net ses.",
    "Gece yarisi listesi hazir bekliyor.",
    "Gece kusu oturumu acik.",
];

/// Inputs evaluated by `GreetingScheduler::maybe_pick`.
#[derive(Debug, Clone, Copy)]
pub struct GreetingContext {
    pub enabled: bool,
    pub is_playing: bool,
    pub window_focused: bool,
    pub now_ms: u64,
    pub local_hour: u32,
    pub lang: ResolvedLang,
    pub rng_index: usize,
}

/// Rate-limited embedded TR/EN greeting scheduler (max 1/hour, suppressed when playing in focus).
#[derive(Debug, Clone, Default)]
pub struct GreetingScheduler {
    last_shown_ms: Option<u64>,
}

impl GreetingScheduler {
    #[must_use]
    pub fn maybe_pick(&mut self, ctx: GreetingContext) -> Option<&'static str> {
        if !ctx.enabled || (ctx.is_playing && ctx.window_focused) {
            return None;
        }
        if let Some(prev) = self.last_shown_ms
            && ctx.now_ms.saturating_sub(prev) < HOUR_MS
        {
            return None;
        }
        let bucket = TimeOfDay::from_hour(ctx.local_hour);
        let pool = phrase_pool(ctx.lang, bucket);
        let phrase = pool[ctx.rng_index % pool.len()];
        self.last_shown_ms = Some(ctx.now_ms);
        Some(phrase)
    }
}

#[must_use]
pub const fn phrase_pool(lang: ResolvedLang, bucket: TimeOfDay) -> &'static [&'static str] {
    match (lang, bucket) {
        (ResolvedLang::En, TimeOfDay::Morning) => EN_MORNING,
        (ResolvedLang::En, TimeOfDay::Afternoon) => EN_AFTERNOON,
        (ResolvedLang::En, TimeOfDay::Evening) => EN_EVENING,
        (ResolvedLang::En, TimeOfDay::Night) => EN_NIGHT,
        (ResolvedLang::Tr, TimeOfDay::Morning) => TR_MORNING,
        (ResolvedLang::Tr, TimeOfDay::Afternoon) => TR_AFTERNOON,
        (ResolvedLang::Tr, TimeOfDay::Evening) => TR_EVENING,
        (ResolvedLang::Tr, TimeOfDay::Night) => TR_NIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_respects_hour_rate_limit_and_playing_focus_suppression() {
        let mut sched = GreetingScheduler::default();
        assert!(
            sched
                .maybe_pick(GreetingContext {
                    enabled: true,
                    is_playing: true,
                    window_focused: true,
                    now_ms: 10_000,
                    local_hour: 9,
                    lang: ResolvedLang::En,
                    rng_index: 0,
                })
                .is_none()
        );
        let g1 = sched
            .maybe_pick(GreetingContext {
                enabled: true,
                is_playing: false,
                window_focused: true,
                now_ms: 10_000,
                local_hour: 9,
                lang: ResolvedLang::En,
                rng_index: 1,
            })
            .expect("first greeting");
        assert!(EN_MORNING.contains(&g1));

        assert!(
            sched
                .maybe_pick(GreetingContext {
                    enabled: true,
                    is_playing: false,
                    window_focused: false,
                    now_ms: 10_000 + 3_599_999,
                    local_hour: 10,
                    lang: ResolvedLang::En,
                    rng_index: 0,
                })
                .is_none()
        );

        let g2 = sched
            .maybe_pick(GreetingContext {
                enabled: true,
                is_playing: false,
                window_focused: false,
                now_ms: 10_000 + 3_600_000,
                local_hour: 20,
                lang: ResolvedLang::Tr,
                rng_index: 2,
            })
            .expect("after 1 hour");
        assert!(TR_EVENING.contains(&g2));

        assert!(
            sched
                .maybe_pick(GreetingContext {
                    enabled: false,
                    is_playing: false,
                    window_focused: false,
                    now_ms: 99_999_999,
                    local_hour: 23,
                    lang: ResolvedLang::Tr,
                    rng_index: 0,
                })
                .is_none()
        );
    }
}
