use std::time::Instant;

pub struct TickMonitor {
    last_tick: u64,
    last_time: Instant,
    initialized: bool,
}

impl TickMonitor {
    pub fn new() -> Self {
        Self {
            last_tick: 0,
            last_time: Instant::now(),
            initialized: false,
        }
    }

    pub fn calculate_pace(&mut self, current_tick: u64) -> f64 {
        if !self.initialized {
            self.last_tick = current_tick;
            self.last_time = Instant::now();
            self.initialized = true;
            return 0.0;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_time).as_secs_f64();
        let tick_delta = current_tick.saturating_sub(self.last_tick);
        
        self.last_tick = current_tick;
        self.last_time = now;

        if elapsed > 0.0 {
            tick_delta as f64 / elapsed
        } else {
            0.0
        }
    }
}
