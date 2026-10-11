use std::{
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::time::Instant;

pub struct RequestGate {
    state: Mutex<(Instant, Instant)>,
    spacing: Duration,
}

impl Default for RequestGate {
    fn default() -> Self {
        Self::new(Duration::from_millis(600))
    }
}

impl RequestGate {
    pub fn new(spacing: Duration) -> Self {
        Self {
            state: Mutex::new((Instant::now(), Instant::now())),
            spacing,
        }
    }

    pub async fn wait(&self) {
        loop {
            let deadline = {
                let mut state = self.state.lock().unwrap();
                let deadline = state.0.max(state.1);
                if deadline <= Instant::now() {
                    state.0 = Instant::now() + self.spacing;
                    return;
                }
                deadline
            };
            tokio::time::sleep_until(deadline).await;
        }
    }

    pub fn defer(&self, delay: Duration) {
        let mut state = self.state.lock().unwrap();
        if let Some(deadline) = Instant::now().checked_add(delay) {
            state.1 = state.1.max(deadline);
        }
    }

    pub fn remaining(&self) -> Duration {
        self.state
            .lock()
            .unwrap()
            .1
            .saturating_duration_since(Instant::now())
    }
}

pub fn shared() -> Arc<RequestGate> {
    static GATE: OnceLock<Arc<RequestGate>> = OnceLock::new();
    GATE.get_or_init(|| Arc::new(RequestGate::default()))
        .clone()
}

pub fn retry_delay(value: Option<&reqwest::header::HeaderValue>) -> Duration {
    let seconds = value
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| {
            *n > 0
                && Instant::now()
                    .checked_add(Duration::from_secs(*n))
                    .is_some()
        });
    Duration::from_secs(seconds.unwrap_or(60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_delays_over_thirty_seconds_and_rolling_limits_are_respected() {
        for (value, expected) in [
            ("57", 57),
            ("3600", 3600),
            ("0", 60),
            ("-1", 60),
            ("broken", 60),
            ("999999999999999999999", 60),
        ] {
            assert_eq!(
                retry_delay(Some(&value.parse().unwrap())),
                Duration::from_secs(expected)
            );
        }
        assert_eq!(retry_delay(None), Duration::from_secs(60));
    }

    #[tokio::test]
    async fn parallel_requests_share_spacing_and_cooldown_and_waits_can_be_cancelled() {
        let gate = RequestGate::new(Duration::from_millis(20));
        let start = Instant::now();
        tokio::join!(gate.wait(), gate.wait(), gate.wait());
        assert!(start.elapsed() >= Duration::from_millis(40));
        gate.defer(Duration::from_secs(57));
        gate.defer(Duration::from_secs(1));
        assert!(gate.remaining() > Duration::from_secs(56));
        assert!(
            tokio::time::timeout(Duration::from_millis(25), gate.wait())
                .await
                .is_err()
        );
    }
}
