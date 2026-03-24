use tokio::time::Duration;

pub struct WorkerBackoff {
    min_delay: f64,
    max_delay: f64,
    current_delay: f64,
    multiplier: f64,
}

impl WorkerBackoff {
    pub const fn new(min_delay: f64, max_delay: f64, multiplier: f64) -> Self {
        Self {
            min_delay,
            max_delay,
            current_delay: min_delay,
            multiplier: multiplier.max(1.0),
        }
    }

    pub const fn reset(&mut self) {
        self.current_delay = self.min_delay;
    }

    pub fn step_and_get_sleep_duration(&mut self) -> Duration {
        let delay_ms = self.current_delay;
        let half_delay = delay_ms / 2.0;

        // Equal jitter: половина фиксированная + половина рандомная
        let jitter = if half_delay > 0.0 {
            rand::random_range(0.0..half_delay)
        } else {
            0.0
        };
        let sleep_ms = (half_delay + jitter).max(1.0);

        // Увеличиваем задержку для следующего вызова
        self.current_delay = (delay_ms * self.multiplier)
            .round()
            .max(self.min_delay)
            .min(self.max_delay);

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Duration::from_millis(sleep_ms as u64)
    }
}
