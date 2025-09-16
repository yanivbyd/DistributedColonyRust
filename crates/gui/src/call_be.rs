#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{BackendRequest, BackendResponse, GetShardImageRequest, GetShardImageResponse, GetShardLayerRequest, GetShardLayerResponse, GetColonyInfoRequest, GetColonyInfoResponse, ShardLayer, Shard, Color, ColonyLifeRules};
use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse, ColonyEventDescription};
use shared::cluster_topology::{ClusterTopology, HostInfo};
use std::net::TcpStream;
use std::io::{Read, Write};
use std::time::Duration;
use bincode;
use crate::connection_pool::ConnectionPool;
use std::sync::OnceLock;

static CONNECTION_POOL: OnceLock<ConnectionPool> = OnceLock::new();

fn get_connection_pool() -> &'static ConnectionPool {
    CONNECTION_POOL.get_or_init(|| ConnectionPool::new())
}

pub fn get_cluster_topology() -> &'static ClusterTopology {
    ClusterTopology::get_instance()
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
    let req = BackendRequest::GetColonyInfo(GetColonyInfoRequest);
    
    let response: BackendResponse = send_request_with_pool(host_info, &req)?;
    if let BackendResponse::GetColonyInfo(GetColonyInfoResponse::Ok { colony_life_rules, current_tick, .. }) = response {
        Some((colony_life_rules, current_tick))
    } else {
        None
    }
}

fn send_coordinator_request(request: &CoordinatorRequest) -> Option<CoordinatorResponse> {
    let coordinator_port = shared::coordinator_api::COORDINATOR_PORT;
    let addr = format!("127.0.0.1:{}", coordinator_port);
    
    let mut stream = TcpStream::connect_timeout(&addr.parse().ok()?, Duration::from_millis(500)).ok()?;
    stream.set_read_timeout(Some(Duration::from_millis(1000))).ok()?;
    stream.set_write_timeout(Some(Duration::from_millis(500))).ok()?;
    
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
    
    bincode::deserialize(&buf).ok()
}

pub fn get_colony_events(limit: usize) -> Option<Vec<ColonyEventDescription>> {
    let req = CoordinatorRequest::GetColonyEvents { limit };
    let response = send_coordinator_request(&req)?;
    if let CoordinatorResponse::GetColonyEventsResponse { events } = response {
        Some(events)
    } else {
        None
    }
}