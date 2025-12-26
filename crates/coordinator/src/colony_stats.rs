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
const TOP_VALUES_LIMIT: usize = 20;

#[derive(Serialize)]
pub struct CreatureStatistics {
    #[serde(rename = "colony_instance_id")]
    pub colony_instance_id: String,
    pub tick: u64,
    #[serde(rename = "creatures_count")]
    pub creatures_count: u64,
    pub histograms: Histograms,
    pub meta: Metadata,
}

#[derive(Serialize)]
pub struct HistogramWithAverage {
    pub distribution: BTreeMap<String, u64>,
    pub average: f64,
    #[serde(rename = "was_cut")]
    pub was_cut: bool,
    #[serde(rename = "unique_values_count")]
    pub unique_values_count: usize,
}

#[derive(Serialize)]
pub struct HistogramWithoutAverage {
    pub distribution: BTreeMap<String, u64>,
    #[serde(rename = "was_cut")]
    pub was_cut: bool,
    #[serde(rename = "unique_values_count")]
    pub unique_values_count: usize,
}

#[derive(Serialize)]
pub struct Histograms {
    #[serde(rename = "health")]
    pub health: HistogramWithAverage,
    #[serde(rename = "creature_size")]
    pub creature_size: HistogramWithAverage,
    #[serde(rename = "can_kill")]
    pub can_kill: HistogramWithAverage,
    #[serde(rename = "can_move")]
    pub can_move: HistogramWithAverage,
    #[serde(rename = "food")]
    pub food: HistogramWithAverage,
    #[serde(rename = "age")]
    pub age: HistogramWithAverage,
    #[serde(rename = "original_color")]
    pub original_color: HistogramWithoutAverage,
}

/// Get all StatMetric variants
/// 
/// This function must include all variants of StatMetric.
/// If a new variant is added to StatMetric, it must be added here.
/// See the test `test_all_stat_metrics_completeness` for validation.
pub fn all_stat_metrics() -> Vec<StatMetric> {
    vec![
        StatMetric::Health,
        StatMetric::Size,
        StatMetric::CanKill,
        StatMetric::CanMove,
        StatMetric::Food,
        StatMetric::Age,
        StatMetric::OriginalColor,
    ]
}

/// Helper function that enumerates all StatMetric variants using an exhaustive match.
/// This will cause a compile error if a new variant is added to StatMetric but not handled here.
/// 
/// This function serves as the source of truth for all variants - if a new variant is added,
/// the compiler will force us to update this match statement.
/// 
/// This is primarily used for testing to ensure all_stat_metrics() includes all variants.
#[allow(dead_code)] // Used in tests/test_colony_stats.rs
pub fn enumerate_all_stat_metric_variants() -> Vec<StatMetric> {
    // Use a dummy value to force exhaustive matching
    // If a new variant is added, this match will fail to compile
    let _exhaustive_check = |m: StatMetric| -> StatMetric {
        match m {
            StatMetric::Health => StatMetric::Health,
            StatMetric::Size => StatMetric::Size,
            StatMetric::CanKill => StatMetric::CanKill,
            StatMetric::CanMove => StatMetric::CanMove,
            StatMetric::Food => StatMetric::Food,
            StatMetric::Age => StatMetric::Age,
            StatMetric::OriginalColor => StatMetric::OriginalColor,
        }
    };
    
    // Return all variants explicitly (must match the match above)
    vec![
        StatMetric::Health,
        StatMetric::Size,
        StatMetric::CanKill,
        StatMetric::CanMove,
        StatMetric::Food,
        StatMetric::Age,
        StatMetric::OriginalColor,
    ]
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
            let tick_str = format_tick_filename(stats.tick);
            if let Err(e) = save_stats_to_disk(&stats, instance_id, &tick_str) {
                log_error!("Failed to save statistics to disk: {}", e);
            } else {
                log!("Successfully saved creature statistics to: {}/{}/stats_shots/{}.json", BASE_BUCKET_DIR, instance_id, tick_str);
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
    
    // Collect histograms for all metrics
    let metrics = all_stat_metrics();
    
    // Helper to map StatMetric to index
    fn metric_id(m: StatMetric) -> u8 {
        match m {
            StatMetric::Health => 0,
            StatMetric::Size => 1,
            StatMetric::CanKill => 2,
            StatMetric::CanMove => 3,
            StatMetric::Food => 4,
            StatMetric::Age => 5,
            StatMetric::OriginalColor => 6,
        }
    }
    
    let mut pos_by_id: BTreeMap<u8, usize> = BTreeMap::new();
    for (idx, m) in metrics.iter().copied().enumerate() {
        pos_by_id.insert(metric_id(m), idx);
    }
    
    let mut missing_shards = Vec::new();
    let mut counts_per_metric: Vec<BTreeMap<i32, u64>> = vec![BTreeMap::new(); metrics.len()];
    let mut string_counts_per_metric: Vec<BTreeMap<String, u64>> = vec![BTreeMap::new(); metrics.len()];
    
    for shard in shards {
        match backend_client::call_backend_get_shard_stats(*shard, metrics.clone()) {
            Some((_tick, per_metric, per_string_metric)) => {
                for (_metric_idx, (metric, buckets)) in per_metric.into_iter().enumerate() {
                    if let Some(&pos) = pos_by_id.get(&metric_id(metric)) {
                        let entry = counts_per_metric.get_mut(pos).unwrap();
                        for b in buckets {
                            *entry.entry(b.value).or_insert(0) += b.occs;
                        }
                    }
                }
                for (_metric_idx, (metric, buckets)) in per_string_metric.into_iter().enumerate() {
                    if let Some(&pos) = pos_by_id.get(&metric_id(metric)) {
                        let entry = string_counts_per_metric.get_mut(pos).unwrap();
                        for b in buckets {
                            *entry.entry(b.value.clone()).or_insert(0) += b.occs;
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
    // Find indices for each metric we want to include in output
    let mut health_idx = None;
    let mut creature_size_idx = None;
    let mut can_kill_idx = None;
    let mut can_move_idx = None;
    let mut food_idx = None;
    let mut age_idx = None;
    let mut original_color_idx = None;
    
    for (idx, metric) in metrics.iter().enumerate() {
        match metric {
            StatMetric::Health => health_idx = Some(idx),
            StatMetric::Size => creature_size_idx = Some(idx),
            StatMetric::CanKill => can_kill_idx = Some(idx),
            StatMetric::CanMove => can_move_idx = Some(idx),
            StatMetric::Food => food_idx = Some(idx),
            StatMetric::Age => age_idx = Some(idx),
            StatMetric::OriginalColor => original_color_idx = Some(idx),
        }
    }
    
    // Calculate creatures_count from the CanKill metric
    let creatures_count = can_kill_idx
        .and_then(|idx| counts_per_metric.get(idx))
        .map(|counts| counts.values().sum::<u64>())
        .unwrap_or(0);
    
    let histograms = Histograms {
        health: health_idx.map(|idx| build_histogram(&counts_per_metric[idx], false)).unwrap_or_else(|| HistogramWithAverage {
            distribution: BTreeMap::new(),
            average: 0.0,
            was_cut: false,
            unique_values_count: 0,
        }),
        creature_size: creature_size_idx.map(|idx| build_histogram(&counts_per_metric[idx], false)).unwrap_or_else(|| HistogramWithAverage {
            distribution: BTreeMap::new(),
            average: 0.0,
            was_cut: false,
            unique_values_count: 0,
        }),
        can_kill: can_kill_idx.map(|idx| build_histogram(&counts_per_metric[idx], true)).unwrap_or_else(|| HistogramWithAverage {
            distribution: BTreeMap::new(),
            average: 0.0,
            was_cut: false,
            unique_values_count: 0,
        }),
        can_move: can_move_idx.map(|idx| build_histogram(&counts_per_metric[idx], true)).unwrap_or_else(|| HistogramWithAverage {
            distribution: BTreeMap::new(),
            average: 0.0,
            was_cut: false,
            unique_values_count: 0,
        }),
        food: food_idx.map(|idx| build_histogram(&counts_per_metric[idx], false)).unwrap_or_else(|| HistogramWithAverage {
            distribution: BTreeMap::new(),
            average: 0.0,
            was_cut: false,
            unique_values_count: 0,
        }),
        age: age_idx.map(|idx| build_histogram(&counts_per_metric[idx], false)).unwrap_or_else(|| HistogramWithAverage {
            distribution: BTreeMap::new(),
            average: 0.0,
            was_cut: false,
            unique_values_count: 0,
        }),
        original_color: original_color_idx.map(|idx| build_string_histogram(&string_counts_per_metric[idx])).unwrap_or_else(|| HistogramWithoutAverage {
            distribution: BTreeMap::new(),
            was_cut: false,
            unique_values_count: 0,
        }),
    };
    
    // Build metadata
    let meta = Metadata {
        created_at_utc: Utc::now().to_rfc3339(),
        colony_width,
        colony_height,
    };
    
    Ok(CreatureStatistics {
        colony_instance_id,
        tick: current_tick,
        creatures_count,
        histograms,
        meta,
    })
}

fn build_histogram(counts: &BTreeMap<i32, u64>, is_boolean: bool) -> HistogramWithAverage {
    // Calculate average
    let mut total_value: i64 = 0;
    let mut total_count: u64 = 0;
    for (&value, &count) in counts.iter() {
        total_value += value as i64 * count as i64;
        total_count += count;
    }
    let average = if total_count > 0 {
        let raw_average = total_value as f64 / total_count as f64;
        // Round to 3 decimal places
        (raw_average * 1000.0).round() / 1000.0
    } else {
        0.0
    };
    
    // Filter: only include counts >= MIN_HISTOGRAM_COUNT, then take top 20 by count
    let mut filtered: Vec<(i32, u64)> = counts
        .iter()
        .filter(|(_, &count)| count >= MIN_HISTOGRAM_COUNT)
        .map(|(&value, &count)| (value, count))
        .collect();
    
    // Record the number of unique values before cutting to top 20
    let unique_values_count = filtered.len();
    let was_cut = unique_values_count > TOP_VALUES_LIMIT;
    
    // Sort by count descending, then take top 20
    filtered.sort_by(|a, b| b.1.cmp(&a.1));
    filtered.truncate(TOP_VALUES_LIMIT);
    
    // Build histogram map
    let mut hist = BTreeMap::new();
    for (value, count) in filtered {
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
    
    HistogramWithAverage {
        distribution: hist,
        average,
        was_cut,
        unique_values_count,
    }
}

fn build_string_histogram(counts: &BTreeMap<String, u64>) -> HistogramWithoutAverage {
    // Filter: only include counts >= MIN_HISTOGRAM_COUNT, then take top 20 by count
    let mut filtered: Vec<(String, u64)> = counts
        .iter()
        .filter(|(_, &count)| count >= MIN_HISTOGRAM_COUNT)
        .map(|(value, &count)| (value.clone(), count))
        .collect();
    
    // Record the number of unique values before cutting to top 20
    let unique_values_count = filtered.len();
    let was_cut = unique_values_count > TOP_VALUES_LIMIT;
    
    // Sort by count descending, then take top 20
    filtered.sort_by(|a, b| b.1.cmp(&a.1));
    filtered.truncate(TOP_VALUES_LIMIT);
    
    // Build histogram map
    let mut hist = BTreeMap::new();
    for (value, count) in filtered {
        hist.insert(value, count);
    }
    
    HistogramWithoutAverage {
        distribution: hist,
        was_cut,
        unique_values_count,
    }
}

/// Format tick number as zero-padded 7-digit string (e.g., 20 -> "0000020")
fn format_tick_filename(tick: u64) -> String {
    format!("{:07}", tick)
}

fn save_stats_to_disk(stats: &CreatureStatistics, instance_id: &str, tick_str: &str) -> Result<(), String> {
    // Build directory path: output/s3/distributed-colony/{id}/stats_shots
    let dir_path = Path::new(BASE_BUCKET_DIR).join(instance_id).join("stats_shots");
    if let Err(e) = std::fs::create_dir_all(&dir_path) {
        return Err(format!("Failed to create directory {}: {}", dir_path.display(), e));
    }
    
    // Construct full file path
    let filename = format!("{}.json", tick_str);
    let file_path = dir_path.join(&filename);
    
    // Serialize to JSON
    let json = serde_json::to_string_pretty(stats)
        .map_err(|e| format!("Failed to serialize statistics to JSON: {}", e))?;
    
    // Write to file
    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write statistics file to {}: {}", file_path.display(), e))?;
    
    Ok(())
}
