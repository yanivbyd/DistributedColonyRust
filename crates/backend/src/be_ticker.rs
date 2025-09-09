use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use shared::metrics::LatencyMonitor;
use shared::utils::new_random_generator;
use rayon::prelude::*;

pub fn start_be_ticker() {
    std::thread::spawn(move || {
        loop {
            if Colony::is_initialized() {
                let mut colony = Colony::instance();
                let current_tick = colony.shards[0].get_current_tick();

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

                if current_tick % 250 == 0 {
                    for shard in &colony.shards {
                        ShardUtils::store_shard(&shard);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    });
}
