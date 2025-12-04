use futures::future::join_all;
use crate::backend_client::send_updated_shard_contents_to_host_async;
use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use shared::utils::new_random_generator;
use shared::cluster_topology::{ClusterTopology, HostInfo};
use crate::backend_config::{get_backend_hostname, get_backend_port};
use std::sync::Arc;

pub fn start_be_ticker() {
    tokio::spawn(async move {
        loop {
            if Colony::is_initialized() {
                let colony = Colony::instance();

                // Get a snapshot of shard keys and Arc handles (cheap clones)
                let (hosted_shards, hosted_colony_shards) = colony.get_hosted_shards();

                // Optional: read current tick from any shard
                let current_tick = {
                    if let Some(first) = hosted_colony_shards.first() {
                        first.lock().unwrap().get_current_tick()
                    } else { 0 }
                };

                let tasks = hosted_colony_shards.iter().map(|shard_arc| {
                    let shard_arc = Arc::clone(shard_arc);
                    tokio::task::spawn_blocking(move || {
                        let mut rng = new_random_generator();
                        let mut shard = shard_arc.lock().unwrap();
                        shard.tick(&mut rng);
                        ShardUtils::export_shard_contents(&shard)
                    })
                });
                let exported = join_all(tasks).await
                    .into_iter().map(|r| r.expect("tick task panicked")).collect::<Vec<_>>();

                let topology = ClusterTopology::get_instance();
                let this_backend_host = HostInfo::new(get_backend_hostname().to_string(), get_backend_port());
                
                // In AWS mode, ClusterTopology should be initialized by coordinator during cloud-start with dynamic topology
                // If we're using static topology (127.0.0.1), skip external backend communication to avoid connecting to non-existent localhost backends
                let is_static_topology = ClusterTopology::is_using_static_topology();
                
                if is_static_topology {
                    // Only log once to avoid spam - this is expected in AWS mode before cloud-start completes
                    static mut LOGGED_WARNING: bool = false;
                    unsafe {
                        if !LOGGED_WARNING {
                            shared::log!("Warning: ClusterTopology using static localhost topology - skipping backend-to-backend shard updates (this is normal before cloud-start completes in AWS mode)");
                            LOGGED_WARNING = true;
                        }
                    }
                }

                for req in &exported {
                    // Update adjacent shards on this backend
                    for shard_key in &hosted_shards {
                        if ShardUtils::is_adjacent_shard(&req.updated_shard, shard_key) {
                            let shard_arc = colony.get_hosted_colony_shard_arc(shard_key).unwrap();
                            let mut shard = shard_arc.lock().unwrap();
                            ShardUtils::updated_shard_contents(&mut shard, req);
                        }
                    }

                    // Send updates to adjacent shards on other backends - only if topology is properly initialized with dynamic topology
                    // In AWS mode, static topology should never be used, so we skip this to avoid connecting to 127.0.0.1:8085
                    if !is_static_topology {
                        let adj: std::collections::HashSet<_> =
                            topology.get_adjacent_shards(&req.updated_shard).into_iter().collect();
                        let hosts = topology.get_backend_hosts_for_shards(&adj.iter().cloned().collect::<Vec<_>>());
                        for host in hosts {
                            if host != this_backend_host {
                                let req_owned = req.clone();
                                tokio::spawn(async move {
                                     let _ = send_updated_shard_contents_to_host_async(&host, &req_owned).await;
                                });
                            }
                        }
                    }
                }

                // optional persistence
                if current_tick % 250 == 0 {
                    for shard_arc in &hosted_colony_shards {
                        let shard = shard_arc.lock().unwrap();
                        ShardUtils::store_shard(&*shard);
                    }
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(25)).await;
        }
    });
}
