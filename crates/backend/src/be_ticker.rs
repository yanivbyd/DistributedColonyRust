use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use shared::metrics::LatencyMonitor;
use shared::utils::new_random_generator;
use shared::cluster_topology::ClusterTopology;
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

                    // Find all adjacent shards that need updating (from all shards in topology)
                    let topology = ClusterTopology::get_instance();
                    let adjacent_shards: std::collections::HashSet<_> = topology.get_adjacent_shards(&req.updated_shard).into_iter().collect();
                    
                    // Get hosts that need to be updated for these adjacent shards
                    let adjacent_shards_vec: Vec<_> = adjacent_shards.iter().cloned().collect();
                    let all_hosts = topology.get_backend_hosts_for_shards(&adjacent_shards_vec);
                    
                    // Get this backend's host (single backend host)
                    let this_backend_host = topology.get_single_backend_host();
                                            
                    // Update local shards that are in the adjacent_shards list
                    for shard in colony.shards.iter_mut() {
                        if adjacent_shards.contains(&shard.shard) {
                            ShardUtils::updated_shard_contents(shard, req);
                        }
                    }
    
                    let external_hosts: Vec<_> = all_hosts.iter()
                        .filter(|host| host != &this_backend_host)
                        .collect();
                    if !external_hosts.is_empty() {
                        panic!("Not implemented yet");
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
