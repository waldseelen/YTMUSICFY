use std::time::Duration;

use tokio::sync::Mutex;
use tokio::time::{Instant, sleep_until};

/// Asynchronous token pacer enforcing a minimum interval between upstream API requests.
#[derive(Debug)]
pub struct PacedLimiter {
    min_interval: Duration,
    next_allowed: Mutex<Option<Instant>>,
}

impl PacedLimiter {
    /// Creates a pacer requiring at least `min_interval` between consecutive completions.
    #[must_use]
    pub fn new(min_interval: Duration) -> Self {
        Self {
            min_interval,
            next_allowed: Mutex::new(None),
        }
    }

    /// Sleeps if necessary until the next request slot is permitted.
    pub async fn acquire(&self) {
        if self.min_interval.is_zero() {
            return;
        }
        let mut guard = self.next_allowed.lock().await;
        let now = Instant::now();
        if let Some(deadline) = *guard
            && deadline > now
        {
            sleep_until(deadline).await;
        }
        *guard = Some(Instant::now() + self.min_interval);
    }
}
