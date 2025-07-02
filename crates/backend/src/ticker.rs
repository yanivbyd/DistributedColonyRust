use crate::colony::ColonySubGrid;
use shared::metrics::LatencyMonitor;

pub fn start_ticker() {
    std::thread::spawn(move || {
        loop {
            if ColonySubGrid::is_initialized() {
                let _monitor = LatencyMonitor::start("tick_latency_ms");
                ColonySubGrid::instance().tick();
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
} 