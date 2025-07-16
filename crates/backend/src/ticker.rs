use crate::colony::Colony;
use shared::metrics::LatencyMonitor;
use shared::log;

pub fn start_ticker() {
    std::thread::spawn(move || {
        let mut tick_count: u64 = 1;
        loop {
            if Colony::is_initialized() {
                if tick_count == 1 || tick_count % 10 == 0 {
                    log!("[BE] Ticker: tick {}", tick_count);
                }

                let mut has_shards = false;
                for colony_shard in Colony::instance().shards.values_mut() {
                    let _monitor = LatencyMonitor::start("tick_latency_ms");
                    colony_shard.tick();
                    has_shards = true;
                }
                if has_shards {
                    tick_count += 1;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
} 