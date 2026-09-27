use std::time::{Duration, Instant};

pub const SAMPLE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct MonitorHealth {
    last_sample: Option<Instant>,
}

impl MonitorHealth {
    pub fn observe(&mut self, sampled_at: Instant, now: Instant) -> bool {
        if sampled_at > now
            || now.duration_since(sampled_at) >= SAMPLE_TIMEOUT
            || self.last_sample.is_some_and(|last| sampled_at <= last)
        {
            return false;
        }
        self.last_sample = Some(sampled_at);
        true
    }

    pub fn ready(&self, now: Instant) -> bool {
        self.last_sample
            .is_some_and(|last| now.saturating_duration_since(last) < SAMPLE_TIMEOUT)
    }

    pub fn invalidate(&mut self) {
        self.last_sample = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_unavailable_expires_and_recovers_only_with_fresh_data() {
        let start = Instant::now();
        let mut health = MonitorHealth::default();
        assert!(!health.ready(start));
        assert!(health.observe(start, start));
        assert!(health.ready(start + Duration::from_millis(200)));
        assert!(!health.ready(start + SAMPLE_TIMEOUT));
        assert!(!health.observe(start, start + SAMPLE_TIMEOUT));
        assert!(health.observe(start + SAMPLE_TIMEOUT, start + SAMPLE_TIMEOUT));
        health.invalidate();
        assert!(!health.ready(start + SAMPLE_TIMEOUT));
    }

    #[test]
    fn delayed_duplicate_and_future_samples_cannot_refresh_watchdog() {
        let start = Instant::now();
        let mut health = MonitorHealth::default();
        assert!(health.observe(start, start));
        assert!(!health.observe(start, start + Duration::from_secs(1)));
        assert!(!health.observe(start + Duration::from_secs(2), start));
        assert!(!health.ready(start + SAMPLE_TIMEOUT));
    }
}
