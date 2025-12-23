use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use shared::{log, log_error};
use shared::be_api::StatMetric;
use crate::coordinator_context::CoordinatorContext;
use crate::backend_client;
use shared::cluster_topology::ClusterTopology;
use chrono::Utc;

const BASE_BUCKET_DIR: &str = "output/s3/distributed-colony";
const MIN_HISTOGRAM_COUNT: u64 = 20;

#[derive(Serialize)]
pub struct CreatureStatistics {
    #[serde(rename = "colony_instance_id")]
    pub colony_instance_id: String,
    pub tick: u64,
    pub histograms: Histograms,
    pub rules: BTreeMap<String, u32>,
    pub meta: Metadata,
}

#[derive(Serialize)]
pub struct Histograms {
    #[serde(rename = "creature_size")]
    pub creature_size: BTreeMap<String, u64>,
    #[serde(rename = "can_kill")]
    pub can_kill: BTreeMap<String, u64>,
    #[serde(rename = "can_move")]
    pub can_move: BTreeMap<String, u64>,
}

#[derive(Serialize)]
pub struct Metadata {
    #[serde(rename = "created_at_utc")]
    pub created_at_utc: String,
    pub colony_width: Option<i32>,
    pub colony_height: Option<i32>,
}

/// Main function to capture colony statistics and save to disk
pub async fn capture_colony_stats() {
    log!("Starting creature statistics capture");
    
    // Get topology
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized, skipping statistics capture");
            return;
        }
    };
    
    // Get all shards
    let shards = topology.get_all_shards();
    if shards.is_empty() {
        log_error!("No shards in topology, skipping statistics capture");
        return;
    }
    
    // Collect statistics
    let stats_result = collect_statistics(&shards).await;
    
    // Save to disk
    match stats_result {
        Ok(stats) => {
            let context = CoordinatorContext::get_instance();
            let stored_info = context.get_coord_stored_info();
            let instance_id = match stored_info.colony_instance_id.as_deref() {
                Some(id) => id,
                None => {
                    log_error!("Colony instance ID is not set, skipping statistics capture");
                    return;
                }
            };
            let timestamp = get_stats_timestamp();
            if let Err(e) = save_stats_to_disk(&stats, instance_id, &timestamp) {
                log_error!("Failed to save statistics to disk: {}", e);
            } else {
                log!("Successfully saved creature statistics to: {}/{}/stats_shots/{}.json", BASE_BUCKET_DIR, instance_id, timestamp);
            }
        }
        Err(e) => {
            log_error!("Failed to collect statistics: {}", e);
        }
    }
}

async fn collect_statistics(
    shards: &[shared::colony_model::Shard],
) -> Result<CreatureStatistics, String> {
    let context = CoordinatorContext::get_instance();

    // Get colony-level info from coordinator storage (ID and dimensions)
    let stored_info = context.get_coord_stored_info();
    let colony_instance_id = stored_info
        .colony_instance_id
        .as_deref()
        .expect("Colony instance ID must be set before capturing stats")
        .to_string();
    let colony_width = stored_info.colony_width;
    let colony_height = stored_info.colony_height;
    drop(stored_info);

    // Get current tick from first available shard
    let current_tick = shards
        .first()
        .and_then(|shard| backend_client::call_backend_for_tick_count(*shard))
        .ok_or_else(|| "Could not get current tick".to_string())?;
    
    // Collect histograms for creature_size, can_kill, can_move
    let metrics = vec![
        StatMetric::CreatureSize,
        StatMetric::CreateCanKill,
        StatMetric::CreateCanMove,
    ];
    
    // Helper to map StatMetric to index
    fn metric_id(m: StatMetric) -> u8 {
        match m {
            StatMetric::Health => 0,
            StatMetric::CreatureSize => 1,
            StatMetric::CreateCanKill => 2,
            StatMetric::CreateCanMove => 3,
            StatMetric::Food => 4,
            StatMetric::Age => 5,
        }
    }
    
    let mut pos_by_id: BTreeMap<u8, usize> = BTreeMap::new();
    for (idx, m) in metrics.iter().copied().enumerate() {
        pos_by_id.insert(metric_id(m), idx);
    }
    
    let mut missing_shards = Vec::new();
    let mut counts_per_metric: Vec<BTreeMap<i32, u64>> = vec![BTreeMap::new(); metrics.len()];
    
    for shard in shards {
        match backend_client::call_backend_get_shard_stats(*shard, metrics.clone()) {
            Some((_tick, per_metric)) => {
                for (_metric_idx, (metric, buckets)) in per_metric.into_iter().enumerate() {
                    if let Some(&pos) = pos_by_id.get(&metric_id(metric)) {
                        let entry = counts_per_metric.get_mut(pos).unwrap();
                        for b in buckets {
                            *entry.entry(b.value).or_insert(0) += b.occs;
                        }
                    }
                }
            }
            None => {
                missing_shards.push(shard.to_id());
            }
        }
    }
    
    // Build histograms with filtering (count >= 20)
    let creature_size_hist = build_histogram(&counts_per_metric[0], false);
    let can_kill_hist = build_histogram(&counts_per_metric[1], true);
    let can_move_hist = build_histogram(&counts_per_metric[2], true);
    
    let histograms = Histograms {
        creature_size: creature_size_hist,
        can_kill: can_kill_hist,
        can_move: can_move_hist,
    };
    
    // Get rules and serialize with human-readable names
    let rules_obj = context.get_colony_life_rules();
    let mut rules = BTreeMap::new();
    rules.insert("Health Cost Per Size Unit".to_string(), rules_obj.health_cost_per_size_unit);
    rules.insert("Eat Capacity Per Size Unit".to_string(), rules_obj.eat_capacity_per_size_unit);
    rules.insert("Health Cost If Can Kill".to_string(), rules_obj.health_cost_if_can_kill);
    rules.insert("Health Cost If Can Move".to_string(), rules_obj.health_cost_if_can_move);
    rules.insert("Mutation Chance".to_string(), rules_obj.mutation_chance);
    rules.insert("Random Death Chance".to_string(), rules_obj.random_death_chance);
    
    // Build metadata
    let meta = Metadata {
        created_at_utc: Utc::now().to_rfc3339(),
        colony_width,
        colony_height,
    };
    
    Ok(CreatureStatistics {
        colony_instance_id,
        tick: current_tick,
        histograms,
        rules,
        meta,
    })
}

fn build_histogram(counts: &BTreeMap<i32, u64>, is_boolean: bool) -> BTreeMap<String, u64> {
    let mut hist = BTreeMap::new();
    
    for (&value, &count) in counts.iter() {
        // Filter: only include counts >= 20
        if count >= MIN_HISTOGRAM_COUNT {
            let key = if is_boolean {
                // Boolean traits: "0" for false, "1" for true
                if value == 0 {
                    "0".to_string()
                } else {
                    "1".to_string()
                }
            } else {
                // Non-boolean traits: use value as string
                value.to_string()
            };
            hist.insert(key, count);
        }
    }
    
    hist
}

fn get_stats_timestamp() -> String {
    let now = chrono::Local::now();
    now.format("%Y_%m_%d__%H_%M_%S").to_string()
}

fn save_stats_to_disk(stats: &CreatureStatistics, instance_id: &str, timestamp: &str) -> Result<(), String> {
    // Build directory path: output/s3/distributed-colony/{id}/stats_shots
    let dir_path = Path::new(BASE_BUCKET_DIR).join(instance_id).join("stats_shots");
    if let Err(e) = std::fs::create_dir_all(&dir_path) {
        return Err(format!("Failed to create directory {}: {}", dir_path.display(), e));
    }
    
    // Construct full file path
    let filename = format!("{}.json", timestamp);
    let file_path = dir_path.join(&filename);
    
    // Serialize to JSON
    let json = serde_json::to_string_pretty(stats)
        .map_err(|e| format!("Failed to serialize statistics to JSON: {}", e))?;
    
    // Write to file
    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write statistics file to {}: {}", file_path.display(), e))?;
    
    Ok(())
}
