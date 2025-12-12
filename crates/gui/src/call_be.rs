#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{BackendRequest, BackendResponse, GetShardImageRequest, GetShardImageResponse, GetShardLayerRequest, GetShardLayerResponse, ShardLayer, Shard, Color, ColonyLifeRules};
use shared::coordinator_api::{ColonyEventDescription, ColonyMetricStats};
use shared::be_api::{StatMetric};
use shared::cluster_topology::{ClusterTopology, HostInfo};
use std::net::TcpStream;
use std::io::{Read, Write};
use std::time::Duration;
use bincode;
use crate::connection_pool::ConnectionPool;
use std::sync::OnceLock;
use shared::ssm;
use shared::cluster_registry::create_cluster_registry;

static CONNECTION_POOL: OnceLock<ConnectionPool> = OnceLock::new();

fn get_connection_pool() -> &'static ConnectionPool {
    CONNECTION_POOL.get_or_init(|| ConnectionPool::new())
}

fn get_shard_endpoint(topology: &ClusterTopology, shard: Shard) -> HostInfo {
    topology.get_host_for_shard(&shard).cloned().expect("Shard not found in cluster topology")
}

fn send_request_with_pool<T>(host_info: &HostInfo, request: &BackendRequest) -> Option<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let pool = get_connection_pool();
    let conn_info = pool.get_connection(host_info)?;
    let mut conn = conn_info.lock().unwrap();
    
    // Get the stream, creating a new connection if needed
    let stream = if let Some(ref mut stream) = conn.stream {
        stream
    } else {
        // Recreate connection if it was closed
        let new_stream = TcpStream::connect_timeout(&host_info.to_address().parse().ok()?, Duration::from_millis(500)).ok()?;
        new_stream.set_read_timeout(Some(Duration::from_millis(1000))).ok()?;
        new_stream.set_write_timeout(Some(Duration::from_millis(500))).ok()?;
        conn.stream = Some(new_stream);
        conn.is_healthy = true;
        conn.stream.as_mut().unwrap()
    };
    
    // Send request
    let encoded = bincode::serialize(request).ok()?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).ok()?;
    stream.write_all(&encoded).ok()?;
    
    // Read response
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).ok()?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).ok()?;
    
    // Update last used time
    conn.last_used = std::time::Instant::now();
    
    bincode::deserialize(&buf).ok()
}

pub fn get_all_shard_retained_images(config: &crate::ShardConfig, topology: &ClusterTopology) -> Vec<Option<RetainedImage>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_retained_image(shard, topology)).collect()
}

fn get_shard_retained_image(shard: Shard, topology: &ClusterTopology) -> Option<RetainedImage> {
    let host_info = get_shard_endpoint(topology, shard);
    let req = BackendRequest::GetShardImage(GetShardImageRequest { shard });
    
    let response: BackendResponse = send_request_with_pool(&host_info, &req)?;
    if let BackendResponse::GetShardImage(GetShardImageResponse::Image { image }) = response {
        let img = color_vec_to_image(&image, shard.width as usize, shard.height as usize);
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

fn get_shard_layer_data(shard: Shard, layer: ShardLayer, topology: &ClusterTopology) -> Option<Vec<i32>> {
    let host_info = get_shard_endpoint(topology, shard);
    let req = BackendRequest::GetShardLayer(GetShardLayerRequest { shard, layer });
    
    let response: BackendResponse = send_request_with_pool(&host_info, &req)?;
    if let BackendResponse::GetShardLayer(GetShardLayerResponse::Ok { data }) = response {
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
    let req = BackendRequest::GetShardImage(GetShardImageRequest { shard });
    
    let response: BackendResponse = send_request_with_pool(&host_info, &req)?;
    if let BackendResponse::GetShardImage(GetShardImageResponse::Image { image }) = response {
        Some(image)
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
            return Some((addr.ip, addr.http_port));
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
            if (backend_addr.ip == host_info.hostname ||
                backend_addr.ip == "127.0.0.1" && host_info.hostname == "127.0.0.1" ||
                backend_addr.ip == "localhost" && host_info.hostname == "localhost") &&
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
