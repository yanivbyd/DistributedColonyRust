use crate::colony::Colony;
use shared::metrics::LatencyMonitor;
use shared::log;
use rayon::prelude::*;

pub fn start_ticker() {
    std::thread::spawn(move || {
        let mut tick_count: u64 = 1;
        loop {
            if Colony::is_initialized() {
                if tick_count == 1 || tick_count % 10 == 0 {
                    log!("[BE] Ticker: tick {}", tick_count);
                }
                let mut colony = Colony::instance();
                if !colony.shards.is_empty() {
                    tick_count += 1;
                }

                colony.shards.par_iter_mut().for_each(|colony_shard| {
                    let _ = LatencyMonitor::start("shard_tick_latency_ms");
                    colony_shard.tick();
                });
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
} 