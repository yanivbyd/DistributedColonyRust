use futures::future::join_all;
use crate::backend_client::send_updated_shard_contents_to_host_async;
use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use shared::utils::new_random_generator;
use shared::cluster_topology::{ClusterTopology, HostInfo};
use shared::log;
use crate::backend_config::{get_backend_hostname, get_backend_port, is_aws_deployment};
use std::sync::{Arc, OnceLock};

static TICKER_STARTED: OnceLock<()> = OnceLock::new();

pub fn start_be_ticker() {
    // Ensure ticker is only started once (idempotent)
    TICKER_STARTED.get_or_init(|| {
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

                let topology = match ClusterTopology::get_instance() {
                    Some(t) => t,
                    None => {
                        log!("Topology not initialized, skipping tick");
                        continue;
                    }
                };
                let this_backend_host = HostInfo::new(get_backend_hostname().to_string(), get_backend_port());

                for req in &exported {
                    for shard_key in &hosted_shards {
                        if ShardUtils::is_adjacent_shard(&req.updated_shard, shard_key) {
                            let shard_arc = colony.get_hosted_colony_shard_arc(shard_key).unwrap();
                            let mut shard = shard_arc.lock().unwrap();
                            ShardUtils::updated_shard_contents(&mut shard, req);
                        }
                    }

                    // external hosts (fire-and-forget)
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

                // optional persistence
                if current_tick % 250 == 0 {
                    for shard_arc in &hosted_colony_shards {
                        let shard = shard_arc.lock().unwrap();
                        ShardUtils::store_shard(&*shard);
                    }
                }
            }

            let sleep_duration = if is_aws_deployment() {
                100
            } else {
                25
            };
            tokio::time::sleep(tokio::time::Duration::from_millis(sleep_duration)).await;
        }
    });
    });
}
