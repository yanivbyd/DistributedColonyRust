use crate::be_be_calls::broadcast_shard_contents_exported;
use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use shared::log;
use shared::metrics::LatencyMonitor;
use rayon::prelude::*;

pub fn start_ticker() {
    std::thread::spawn(move || {
        let mut tick_count: u64 = 1;
        let rt = tokio::runtime::Runtime::new().unwrap();
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

            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
} 