use crate::colony::{Colony};
use shared::metrics::LatencyMonitor;
use shared::log;

pub fn start_ticker() {
    std::thread::spawn(move || {
        let mut tick_count: u64 = 1;
        loop {
            if Colony::is_initialized() {
                let _monitor = LatencyMonitor::start("tick_latency_ms");
                if tick_count == 1 || tick_count % 10 == 0 {
                    log!("[BE] Ticker: tick {}", tick_count);
                }
                tick_count += 1;
                Colony::instance().shard.as_mut().unwrap().tick();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
} 