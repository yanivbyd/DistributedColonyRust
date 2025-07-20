use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use crate::colony_events::{apply_event, log_event, randomize_event};
use shared::log;
use shared::metrics::LatencyMonitor;
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

                // Prepare a vector to hold the exported contents
                let exported_contents: Vec<_> = colony.shards.par_iter_mut()
                    .map(|colony_shard| {
                        let _ = LatencyMonitor::start("shard_tick_latency_ms");
                        colony_shard.tick();
                        ShardUtils::export_shard_contents(colony_shard)
                    })
                    .collect();

                for req in &exported_contents {
                    for shard in colony.shards.iter_mut() {
                        ShardUtils::updated_shard_contents(shard, req);
                    }
                }

                // Randomize event and apply it (locally)
                if let Some(event) = randomize_event(&colony) {
                    log_event(&event);
                    apply_event(&mut colony, &event);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
} 