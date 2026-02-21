use rand::Rng;
use tokio::time::Duration;

pub struct WorkerBackoff {
    min_delay: Duration,
    max_delay: Duration,
    current_delay: Duration,
    multiplier: f64,
}

impl WorkerBackoff {
    pub fn new(min_ms: u64, max_ms: u64, multiplier: f64) -> Self {
        Self {
            min_delay: Duration::from_millis(min_ms),
            max_delay: Duration::from_millis(max_ms),
            current_delay: Duration::from_millis(min_ms),
            multiplier,
        }
    }

    pub fn reset(&mut self) {
        self.current_delay = self.min_delay;
    }

    pub fn step_and_get_sleep_duration(&mut self) -> Duration {
        let delay_ms = self.current_delay.as_millis() as u64;
        let half_delay = delay_ms / 2;

        // Equal jitter: половина фиксированная + половина рандомная
        let jitter = if half_delay > 0 {
            rand::rng().random_range(0..half_delay)
        } else {
            0
        };
        let sleep_ms = half_delay.saturating_add(jitter).max(1);

        // Увеличиваем задержку для следующего вызова
        let next_delay_ms = ((delay_ms as f64) * self.multiplier)
            .round()
            .max(self.min_delay.as_millis() as f64) as u64;
        self.current_delay = Duration::from_millis(next_delay_ms).min(self.max_delay);

        Duration::from_millis(sleep_ms)
    }
}
