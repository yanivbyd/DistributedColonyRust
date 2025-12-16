#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{ShardLayer, Shard, Color, ColonyLifeRules};
use shared::coordinator_api::{ColonyEventDescription, ColonyMetricStats};
use shared::be_api::{StatMetric};
use shared::cluster_topology::{ClusterTopology, HostInfo};
use std::time::Duration;
use shared::ssm;
use shared::cluster_registry::create_cluster_registry;

fn get_shard_endpoint(topology: &ClusterTopology, shard: Shard) -> HostInfo {
    topology.get_host_for_shard(&shard).cloned().expect("Shard not found in cluster topology")
}

pub fn get_all_shard_retained_images(config: &crate::ShardConfig, topology: &ClusterTopology) -> Vec<Option<RetainedImage>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_retained_image(shard, topology)).collect()
}

fn get_shard_retained_image(shard: Shard, topology: &ClusterTopology) -> Option<RetainedImage> {
    let host_info = get_shard_endpoint(topology, shard);
    let http_port = get_backend_http_port(&host_info)?;
    let shard_id = shard.to_id();
    
    let url = format!("http://{}:{}/api/shard/{}/image", host_info.hostname, http_port, shard_id);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;
    
    let response = client.get(&url).send().ok()?;
    
    if response.status().is_success() {
        let rgb_bytes = response.bytes().ok()?;
        let width = shard.width as usize;
        let height = shard.height as usize;
        
        // Convert raw RGB bytes to Vec<Color>
        if rgb_bytes.len() != width * height * 3 {
            return None;
        }
        
        let mut colors = Vec::with_capacity(width * height);
        for chunk in rgb_bytes.chunks_exact(3) {
            colors.push(Color {
                red: chunk[0],
                green: chunk[1],
                blue: chunk[2],
            });
        }
        
        let img = color_vec_to_image(&colors, width, height);
        Some(RetainedImage::from_color_image("colony_shard", img))
    } else {
        None
    }
}

fn color_vec_to_image(colors: &[Color], width: usize, height: usize) -> egui::ColorImage {
    let mut img = egui::ColorImage::new([width, height], egui::Color32::BLACK);
    for (i, color) in colors.iter().enumerate() {
        let x = i % width;
        let y = i / width;
        if x < width && y < height {
            img.pixels[y * width + x] = egui::Color32::from_rgb(color.red, color.green, color.blue);
        }
    }
    img
}

pub fn get_all_shard_layer_data(layer: ShardLayer, config: &crate::ShardConfig, topology: &ClusterTopology) -> Vec<Option<Vec<i32>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_layer_data(shard, layer, topology)).collect()
}

fn shard_layer_to_kebab_case(layer: ShardLayer) -> &'static str {
    match layer {
        ShardLayer::CreatureSize => "creature-size",
        ShardLayer::ExtraFood => "extra-food",
        ShardLayer::CanKill => "can-kill",
        ShardLayer::CanMove => "can-move",
        ShardLayer::CostPerTurn => "cost-per-turn",
        ShardLayer::Food => "food",
        ShardLayer::Health => "health",
        ShardLayer::Age => "age",
    }
}

fn get_shard_layer_data(shard: Shard, layer: ShardLayer, topology: &ClusterTopology) -> Option<Vec<i32>> {
    let host_info = get_shard_endpoint(topology, shard);
    let http_port = get_backend_http_port(&host_info)?;
    let shard_id = shard.to_id();
    let layer_name = shard_layer_to_kebab_case(layer);
    
    let url = format!("http://{}:{}/api/shard/{}/layer/{}", host_info.hostname, http_port, shard_id, layer_name);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;
    
    let response = client.get(&url).send().ok()?;
    
    if response.status().is_success() {
        let binary_data = response.bytes().ok()?;
        
        // Parse binary format: length (u32 LE) + i32 values (LE)
        if binary_data.len() < 4 {
            return None;
        }
        
        let count = u32::from_le_bytes([
            binary_data[0],
            binary_data[1],
            binary_data[2],
            binary_data[3],
        ]) as usize;
        
        if binary_data.len() != 4 + count * 4 {
            return None;
        }
        
        let mut data = Vec::with_capacity(count);
        for i in 0..count {
            let offset = 4 + i * 4;
            let value = i32::from_le_bytes([
                binary_data[offset],
                binary_data[offset + 1],
                binary_data[offset + 2],
                binary_data[offset + 3],
            ]);
            data.push(value);
        }
        
        Some(data)
    } else {
        None
    }
}

pub fn get_all_shard_color_data(config: &crate::ShardConfig, topology: &ClusterTopology) -> Vec<Option<Vec<Color>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_color_data(shard, topology)).collect()
}

fn get_shard_color_data(shard: Shard, topology: &ClusterTopology) -> Option<Vec<Color>> {
    let host_info = get_shard_endpoint(topology, shard);
    let http_port = get_backend_http_port(&host_info)?;
    let shard_id = shard.to_id();
    
    let url = format!("http://{}:{}/api/shard/{}/image", host_info.hostname, http_port, shard_id);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;
    
    let response = client.get(&url).send().ok()?;
    
    if response.status().is_success() {
        let rgb_bytes = response.bytes().ok()?;
        let width = shard.width as usize;
        let height = shard.height as usize;
        
        // Convert raw RGB bytes to Vec<Color>
        if rgb_bytes.len() != width * height * 3 {
            return None;
        }
        
        let mut colors = Vec::with_capacity(width * height);
        for chunk in rgb_bytes.chunks_exact(3) {
            colors.push(Color {
                red: chunk[0],
                green: chunk[1],
                blue: chunk[2],
            });
        }
        
        Some(colors)
    } else {
        None
    }
}

pub fn get_colony_info(topology: &ClusterTopology) -> Option<(Option<ColonyLifeRules>, Option<u64>)> {
    // Get the first available backend host
    let backend_hosts = topology.get_all_backend_hosts();
    if backend_hosts.is_empty() {
        return None;
    }
    
    let host_info = &backend_hosts[0];
    let http_port = get_backend_http_port(host_info)?;
    
    let url = format!("http://{}:{}/api/colony-info", host_info.hostname, http_port);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;
    
    let response = client.get(&url).send().ok()?;
    
    if response.status().is_success() {
        #[derive(serde::Deserialize)]
        struct Response {
            #[serde(rename = "width")]
            _width: i32,
            #[serde(rename = "height")]
            _height: i32,
            #[serde(rename = "shards")]
            _shards: Vec<Shard>,
            colony_life_rules: Option<ColonyLifeRules>,
            current_tick: Option<u64>,
        }
        
        let resp_data = response.json::<Response>().ok()?;
        Some((resp_data.colony_life_rules, resp_data.current_tick))
    } else {
        None
    }
}

fn get_coordinator_http_info() -> Option<(String, u16)> {
    // Try to discover coordinator HTTP info using SSM
    // We need to determine the deployment mode - try both localhost and aws
    for mode in &["localhost", "aws"] {
        let _registry = create_cluster_registry(mode);
        let rt = tokio::runtime::Runtime::new().ok()?;
        if let Some(addr) = rt.block_on(ssm::discover_coordinator()) {
            return Some((addr.public_ip, addr.http_port));
        }
    }
    None
}

fn get_backend_http_port(host_info: &HostInfo) -> Option<u16> {
    // Try to discover backend HTTP port using SSM
    for mode in &["localhost", "aws"] {
        let _registry = create_cluster_registry(mode);
        let rt = tokio::runtime::Runtime::new().ok()?;
        let backend_addresses = rt.block_on(ssm::discover_backends());
        for backend_addr in backend_addresses {
            if (backend_addr.private_ip == host_info.hostname ||
                backend_addr.private_ip == "127.0.0.1" && host_info.hostname == "127.0.0.1" ||
                backend_addr.private_ip == "localhost" && host_info.hostname == "localhost") &&
               backend_addr.internal_port == host_info.port {
                return Some(backend_addr.http_port);
            }
        }
    }
    None
}

pub fn get_colony_events(limit: usize) -> Option<Vec<ColonyEventDescription>> {
    let (coordinator_host, http_port) = get_coordinator_http_info()?;
    
    let url = format!("http://{}:{}/api/colony-events?limit={}", coordinator_host, http_port, limit);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;
    
    let response = client.get(&url).send().ok()?;
    
    if response.status().is_success() {
        #[derive(serde::Deserialize)]
        struct Response {
            events: Vec<ColonyEventDescription>,
        }
        response.json::<Response>().ok().map(|r| r.events)
    } else {
        None
    }
}

pub fn get_colony_stats(metrics: Vec<StatMetric>) -> Option<(u64, Vec<ColonyMetricStats>)> {
    let (coordinator_host, http_port) = get_coordinator_http_info()?;
    
    // Convert StatMetric enum to string
    let metric_strings: Vec<String> = metrics.iter().map(|m| {
        match m {
            StatMetric::Health => "Health",
            StatMetric::CreatureSize => "CreatureSize",
            StatMetric::CreateCanKill => "CreateCanKill",
            StatMetric::CreateCanMove => "CreateCanMove",
            StatMetric::Food => "Food",
            StatMetric::Age => "Age",
        }.to_string()
    }).collect();
    
    let url = format!("http://{}:{}/api/colony-stats", coordinator_host, http_port);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;
    
    #[derive(serde::Serialize)]
    struct Request {
        metrics: Vec<String>,
    }
    
    let request_body = Request { metrics: metric_strings };
    let response = client.post(&url).json(&request_body).send().ok()?;
    
    if response.status().is_success() {
        #[derive(serde::Deserialize)]
        struct MetricResponse {
            metric: String,
            avg: f64,
            buckets: Vec<shared::be_api::StatBucket>,
        }
        
        #[derive(serde::Deserialize)]
        struct Response {
            tick_count: u64,
            metrics: Vec<MetricResponse>,
        }
        
        let resp_data = response.json::<Response>().ok()?;
        
        // Convert back to StatMetric enum
        let metric_stats: Vec<ColonyMetricStats> = resp_data.metrics.into_iter().map(|m| {
            let metric = match m.metric.as_str() {
                "Health" => StatMetric::Health,
                "CreatureSize" => StatMetric::CreatureSize,
                "CreateCanKill" => StatMetric::CreateCanKill,
                "CreateCanMove" => StatMetric::CreateCanMove,
                "Food" => StatMetric::Food,
                "Age" => StatMetric::Age,
                _ => StatMetric::Health, // Default fallback
            };
            ColonyMetricStats {
                metric,
                avg: m.avg,
                buckets: m.buckets,
            }
        }).collect();
        
        Some((resp_data.tick_count, metric_stats))
    } else {
        None
    }
}
