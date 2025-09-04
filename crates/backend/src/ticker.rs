use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use crate::colony_events::{apply_event, log_event, randomize_event};
use shared::log;
use shared::metrics::LatencyMonitor;
use shared::utils::new_random_generator;
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

                // First phase: tick all shards in parallel
                colony.shards.par_iter_mut().for_each(|shard| {
                    let _ = LatencyMonitor::start("shard_tick_latency_ms");
                    let mut rng = new_random_generator();
                    shard.tick(&mut rng);
                });
                    
                // Export all shard contents in parallel
                let exported_contents: Vec<_> = colony.shards.par_iter()
                    .map(|colony_shard| ShardUtils::export_shard_contents(colony_shard))
                    .collect();

                // Update shards with adjacent exported contents
                for req in &exported_contents {
                    for shard in colony.shards.iter_mut() {
                        if ShardUtils::are_shards_adjacent(&req.updated_shard, &shard.shard) {
                            ShardUtils::updated_shard_contents(shard, req);
                        }
                    }
                }

                // Randomize event and apply it (locally)
                let mut event_rng = new_random_generator();
                if let Some(event) = randomize_event(&colony, &mut event_rng) {
                    log_event(&event);
                    apply_event(&mut colony, &event);
                }

                if tick_count % 100 == 0 {
                    for shard in &colony.shards {
                        ShardUtils::store_shard(&shard);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
} 