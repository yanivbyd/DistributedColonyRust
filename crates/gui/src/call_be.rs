#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{ShardLayer, Shard, Color, ColonyLifeRules};
use shared::coordinator_api::{ColonyEventDescription, ColonyMetricStats};
use shared::be_api::StatMetric;
use shared::cluster_topology::{ClusterTopology, HostInfo};
use std::time::{Duration, Instant};
use std::sync::Arc;
use shared::ssm;
use shared::cluster_registry::create_cluster_registry;
use crate::latency_tracker::{LatencyTracker, OperationKey, OperationType};
use shared::{log, log_error};

fn get_shard_endpoint(topology: &ClusterTopology, shard: Shard) -> HostInfo {
    topology.get_host_for_shard(&shard).cloned().expect("Shard not found in cluster topology")
}

pub fn get_all_shard_retained_images(config: &crate::ShardConfig, topology: &ClusterTopology, latency_tracker: &Arc<LatencyTracker>, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Vec<Option<RetainedImage>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_retained_image(shard, topology, latency_tracker, backend_http_ports)).collect()
}

fn get_shard_retained_image(shard: Shard, topology: &ClusterTopology, latency_tracker: &Arc<LatencyTracker>, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Option<RetainedImage> {
    let host_info = get_shard_endpoint(topology, shard);
    let (public_ip, http_port) = get_backend_http_info(&host_info, backend_http_ports)?;
    let shard_id = shard.to_id();

    let url = format!("http://{}:{}/api/shard/{}/image", public_ip, http_port, shard_id);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;

    let start = Instant::now();
    let response_result = client.get(&url).send();
    let latency = start.elapsed();

    let key = OperationKey::new(OperationType::GetShardImage, host_info.clone());

    let response = match response_result {
        Ok(r) => {
            latency_tracker.record_success(key, latency);
            r
        },
        Err(e) => {
            latency_tracker.record_error(key);
            log_error!("GUI HTTP error: operation=GetShardImage, host={}:{}, url={}, duration_ms={:.2}, error={}",
                       host_info.hostname, host_info.port, url, latency.as_secs_f64() * 1000.0, e);
            return None;
        }
    };
    
    if response.status().is_success() {
        let content_length = response.content_length().unwrap_or(0);
        let content_encoding = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("identity")
            .to_string();
        let rgb_bytes = match response.bytes() {
            Ok(bytes) => bytes,
            Err(e) => {
                log_error!(
                    "GUI HTTP error reading body: operation=GetShardImage, host={}:{}, url={}, content_encoding={}, error={}",
                    host_info.hostname,
                    host_info.port,
                    url,
                    content_encoding,
                    e
                );
                return None;
            }
        };
        let width = shard.width as usize;
        let height = shard.height as usize;
        
        // Convert raw RGB bytes to Vec<Color>
        if rgb_bytes.len() != width * height * 3 {
            log_error!(
                "GUI HTTP shard image size mismatch: shard_id={}, host={}:{}, url={}, expected_bytes={}, actual_bytes={}, content_length={}, content_encoding={}",
                shard_id,
                host_info.hostname,
                host_info.port,
                url,
                width * height * 3,
                rgb_bytes.len(),
                content_length,
                content_encoding
            );
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
        log!(
            "GUI HTTP success: operation=GetShardImage, shard_id={}, host={}:{}, url={}, duration_ms={:.2}, bytes_received={}, content_length={}, content_encoding={}",
            shard_id,
            host_info.hostname,
            host_info.port,
            url,
            latency.as_secs_f64() * 1000.0,
            rgb_bytes.len(),
            content_length,
            content_encoding
        );
        Some(RetainedImage::from_color_image("colony_shard", img))
    } else {
        let status = response.status();
        log_error!(
            "GUI HTTP non-success status for shard image: shard_id={}, host={}:{}, url={}, status_code={}",
            shard_id,
            host_info.hostname,
            host_info.port,
            url,
            status.as_u16()
        );
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

pub fn get_all_shard_layer_data(layer: ShardLayer, config: &crate::ShardConfig, topology: &ClusterTopology, latency_tracker: &Arc<LatencyTracker>, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Vec<Option<Vec<i32>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_layer_data(shard, layer, topology, latency_tracker, backend_http_ports)).collect()
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

fn get_shard_layer_data(shard: Shard, layer: ShardLayer, topology: &ClusterTopology, latency_tracker: &Arc<LatencyTracker>, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Option<Vec<i32>> {
    let host_info = get_shard_endpoint(topology, shard);
    let (public_ip, http_port) = get_backend_http_info(&host_info, backend_http_ports)?;
    let shard_id = shard.to_id();
    let layer_name = shard_layer_to_kebab_case(layer);

    let url = format!("http://{}:{}/api/shard/{}/layer/{}", public_ip, http_port, shard_id, layer_name);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;

    let start = Instant::now();
    let response = client.get(&url).send();
    let latency = start.elapsed();
    let latency_ms = latency.as_millis() as f64;

    let key = OperationKey::new(OperationType::GetShardLayer, host_info.clone());

    let response = match response {
        Ok(r) => {
            latency_tracker.record_success(key.clone(), latency);
            let avg_latency = latency_tracker.get_node_stats(&host_info).avg_latency_ms.unwrap_or(0.0);
            log!("GUI HTTP success: operation=GetShardLayer, host={}:{}, url={}, duration_ms={:.2}, avg_latency_ms={:.2}",
                 host_info.hostname, host_info.port, url, latency_ms, avg_latency);
            r
        },
        Err(e) => {
            latency_tracker.record_error(key.clone());
            // Check if it's a timeout (1500ms timeout)
            let is_timeout = latency_ms >= 1500.0 || e.is_timeout();
            let avg_latency = latency_tracker.get_node_stats(&host_info).avg_latency_ms.unwrap_or(0.0);
            if is_timeout {
                log_error!("GUI HTTP timeout: operation=GetShardLayer, host={}:{}, url={}, duration_ms={:.2}, avg_latency_ms={:.2}", 
                          host_info.hostname, host_info.port, url, latency_ms, avg_latency);
            } else {
                log_error!("GUI HTTP error: operation=GetShardLayer, host={}:{}, url={}, duration_ms={:.2}, avg_latency_ms={:.2}, error={}", 
                          host_info.hostname, host_info.port, url, latency_ms, avg_latency, e);
            }
            return None;
        }
    };
    
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

pub fn get_all_shard_color_data(config: &crate::ShardConfig, topology: &ClusterTopology, latency_tracker: &Arc<LatencyTracker>, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Vec<Option<Vec<Color>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_color_data(shard, topology, latency_tracker, backend_http_ports)).collect()
}

fn get_shard_color_data(shard: Shard, topology: &ClusterTopology, latency_tracker: &Arc<LatencyTracker>, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Option<Vec<Color>> {
    let host_info = get_shard_endpoint(topology, shard);
    let (public_ip, http_port) = get_backend_http_info(&host_info, backend_http_ports)?;
    let shard_id = shard.to_id();

    let url = format!("http://{}:{}/api/shard/{}/image", public_ip, http_port, shard_id);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(1500))
        .build()
        .ok()?;

    let start = Instant::now();
    let response_result = client.get(&url).send();
    let latency = start.elapsed();
    let latency_ms = latency.as_millis() as f64;

    let key = OperationKey::new(OperationType::GetShardImage, host_info.clone());

    let response = match response_result {
        Ok(r) => {
            latency_tracker.record_success(key.clone(), latency);
            let avg_latency = latency_tracker.get_node_stats(&host_info).avg_latency_ms.unwrap_or(0.0);
            let content_length = r.content_length().unwrap_or(0);
            log!(
                "GUI HTTP success: operation=GetShardImage, host={}:{}, url={}, duration_ms={:.2}, avg_latency_ms={:.2}, content_length={}",
                host_info.hostname,
                host_info.port,
                url,
                latency_ms,
                avg_latency,
                content_length
            );
            r
        },
        Err(e) => {
            latency_tracker.record_error(key.clone());
            // Check if it's a timeout (1500ms timeout)
            let is_timeout = latency_ms >= 1500.0 || e.is_timeout();
            let avg_latency = latency_tracker.get_node_stats(&host_info).avg_latency_ms.unwrap_or(0.0);
            if is_timeout {
                log_error!("GUI HTTP timeout: operation=GetShardImage, host={}:{}, url={}, duration_ms={:.2}, avg_latency_ms={:.2}", 
                          host_info.hostname, host_info.port, url, latency_ms, avg_latency);
            } else {
                log_error!("GUI HTTP error: operation=GetShardImage, host={}:{}, url={}, duration_ms={:.2}, avg_latency_ms={:.2}, error={}", 
                          host_info.hostname, host_info.port, url, latency_ms, avg_latency, e);
            }
            return None;
        }
    };
    
    if response.status().is_success() {
        let content_length = response.content_length().unwrap_or(0);
        let content_encoding = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("identity")
            .to_string();
        let rgb_bytes = match response.bytes() {
            Ok(bytes) => bytes,
            Err(e) => {
                log_error!(
                    "GUI HTTP error reading body: operation=GetShardImage, host={}:{}, url={}, content_encoding={}, error={}",
                    host_info.hostname,
                    host_info.port,
                    url,
                    content_encoding,
                    e
                );
                return None;
            }
        };
        let width = shard.width as usize;
        let height = shard.height as usize;
        
        // Convert raw RGB bytes to Vec<Color>
        if rgb_bytes.len() != width * height * 3 {
            log_error!(
                "GUI HTTP shard color data size mismatch: shard_id={}, host={}:{}, url={}, expected_bytes={}, actual_bytes={}, content_length={}, content_encoding={}",
                shard_id,
                host_info.hostname,
                host_info.port,
                url,
                width * height * 3,
                rgb_bytes.len(),
                content_length,
                content_encoding
            );
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
        let status = response.status();
        log_error!(
            "GUI HTTP non-success status for shard color data: shard_id={}, host={}:{}, url={}, status_code={}",
            shard_id,
            host_info.hostname,
            host_info.port,
            url,
            status.as_u16()
        );
        None
    }
}

pub fn get_colony_info(topology: &ClusterTopology, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Option<(Option<ColonyLifeRules>, Option<u64>)> {
    // Get the first available backend host
    let backend_hosts = topology.get_all_backend_hosts();
    if backend_hosts.is_empty() {
        return None;
    }
    
    let host_info = &backend_hosts[0];
    let (public_ip, http_port) = get_backend_http_info(host_info, backend_http_ports)?;
    
    let url = format!("http://{}:{}/api/colony-info", public_ip, http_port);
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

fn get_coordinator_http_info(cached_coordinator_http_port: Option<u16>, deployment_mode: &str) -> Option<(String, u16)> {
    // Use cached HTTP port if available (colony already started)
    if let Some(http_port) = cached_coordinator_http_port {
        // For localhost, use 127.0.0.1; for AWS, we still need to discover the public IP
        // But since we have the port, try localhost first
        if deployment_mode == "localhost" {
            return Some(("127.0.0.1".to_string(), http_port));
        }
        // For AWS, fall through to SSM discovery for public IP (but this should be rare)
    }
    
    // Fallback: Try to discover coordinator HTTP info using SSM (only if not cached)
    for mode in &["localhost", "aws"] {
        let _registry = create_cluster_registry(mode);
        let rt = tokio::runtime::Runtime::new().ok()?;
        if let Some(addr) = rt.block_on(ssm::discover_coordinator()) {
            return Some((addr.public_ip, addr.http_port));
        }
    }
    None
}

fn get_backend_http_info(host_info: &HostInfo, backend_http_ports: &std::collections::HashMap<HostInfo, u16>) -> Option<(String, u16)> {
    // Use cached HTTP port if available (colony already started)
    if let Some(http_port) = backend_http_ports.get(host_info) {
        // For localhost, use 127.0.0.1; for AWS, we still need the public IP
        // Check if hostname is localhost/127.0.0.1
        if host_info.hostname == "127.0.0.1" || host_info.hostname == "localhost" {
            return Some((host_info.hostname.clone(), *http_port));
        }
        // For AWS with private IP, we need public IP - fall through to SSM discovery
        // But this should be rare once colony is started
    }
    
    // Fallback: Try to discover backend HTTP info (public IP and port) using SSM (only if not cached)
    for mode in &["localhost", "aws"] {
        let _registry = create_cluster_registry(mode);
        let rt = tokio::runtime::Runtime::new().ok()?;
        let backend_addresses = rt.block_on(ssm::discover_backends());
        for backend_addr in backend_addresses {
            if (backend_addr.private_ip == host_info.hostname ||
                backend_addr.private_ip == "127.0.0.1" && host_info.hostname == "127.0.0.1" ||
                backend_addr.private_ip == "localhost" && host_info.hostname == "localhost") &&
               backend_addr.internal_port == host_info.port {
                return Some((backend_addr.public_ip, backend_addr.http_port));
            }
        }
    }
    None
}

pub fn get_colony_events(limit: usize, coordinator_http_port: Option<u16>, deployment_mode: &str) -> Option<Vec<ColonyEventDescription>> {
    let (coordinator_host, http_port) = get_coordinator_http_info(coordinator_http_port, deployment_mode)?;
    
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

pub fn get_colony_stats(metrics: Vec<StatMetric>, coordinator_http_port: Option<u16>, deployment_mode: &str) -> Option<(u64, Vec<ColonyMetricStats>)> {
    let (coordinator_host, http_port) = get_coordinator_http_info(coordinator_http_port, deployment_mode)?;
    
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
