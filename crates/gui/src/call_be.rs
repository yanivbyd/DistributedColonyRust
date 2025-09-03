#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{BackendRequest, BackendResponse, GetShardImageRequest, GetShardImageResponse, GetShardLayerRequest, GetShardLayerResponse, ShardLayer, Shard, Color};
use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse};
use std::net::TcpStream;
use std::io::{Read, Write};
use std::collections::HashMap;
use bincode;

const COORDINATOR_PORT: u16 = 8083;

pub fn fetch_routing_table_from_coordinator() -> HashMap<Shard, (String, u16)> {
    let addr = format!("127.0.0.1:{}", COORDINATOR_PORT);
    let mut stream = TcpStream::connect(&addr).expect("Failed to connect to coordinator");
    
    let req = CoordinatorRequest::GetRoutingTable;
    let encoded = bincode::serialize(&req).expect("Failed to serialize request");
    let len = (encoded.len() as u32).to_be_bytes();
    
    stream.write_all(&len).expect("Failed to write request length");
    stream.write_all(&encoded).expect("Failed to write request");
    
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).expect("Failed to read response length");
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).expect("Failed to read response body");
    
    let response: CoordinatorResponse = bincode::deserialize(&buf).expect("Failed to deserialize response");
    
    let CoordinatorResponse::GetRoutingTableResponse { entries } = response;
    let mut routing_map = HashMap::new();
    for entry in entries {
        routing_map.insert(entry.shard, (entry.hostname, entry.port));
    }
    routing_map
}

fn get_shard_endpoint(routing_table: &HashMap<Shard, (String, u16)>, shard: Shard) -> (String, u16) {
    routing_table.get(&shard).cloned().expect("Shard not found in routing table")
}

pub fn get_all_shard_retained_images(config: &crate::ShardConfig, routing_table: &HashMap<Shard, (String, u16)>) -> Vec<Option<RetainedImage>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_retained_image(shard, routing_table)).collect()
}

fn get_shard_retained_image(shard: Shard, routing_table: &HashMap<Shard, (String, u16)>) -> Option<RetainedImage> {
    let (hostname, port) = get_shard_endpoint(routing_table, shard);
    let addr = format!("{}:{}", hostname, port);
    let mut stream = TcpStream::connect(&addr).ok()?;
    let req = BackendRequest::GetShardImage(GetShardImageRequest { shard });
    let encoded = bincode::serialize(&req).ok()?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).ok()?;
    stream.write_all(&encoded).ok()?;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).ok()?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).ok()?;
    let response: BackendResponse = bincode::deserialize(&buf).ok()?;
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

pub fn get_all_shard_layer_data(layer: ShardLayer, config: &crate::ShardConfig, routing_table: &HashMap<Shard, (String, u16)>) -> Vec<Option<Vec<i32>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_layer_data(shard, layer, routing_table)).collect()
}

fn get_shard_layer_data(shard: Shard, layer: ShardLayer, routing_table: &HashMap<Shard, (String, u16)>) -> Option<Vec<i32>> {
    let (hostname, port) = get_shard_endpoint(routing_table, shard);
    let addr = format!("{}:{}", hostname, port);
    let mut stream = TcpStream::connect(&addr).ok()?;
    let req = BackendRequest::GetShardLayer(GetShardLayerRequest { shard, layer });
    let encoded = bincode::serialize(&req).ok()?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).ok()?;
    stream.write_all(&encoded).ok()?;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).ok()?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).ok()?;
    let response: BackendResponse = bincode::deserialize(&buf).ok()?;
    if let BackendResponse::GetShardLayer(GetShardLayerResponse::Ok { data }) = response {
        Some(data)
    } else {
        None
    }
}

pub fn get_all_shard_color_data(config: &crate::ShardConfig, routing_table: &HashMap<Shard, (String, u16)>) -> Vec<Option<Vec<Color>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_color_data(shard, routing_table)).collect()
}

fn get_shard_color_data(shard: Shard, routing_table: &HashMap<Shard, (String, u16)>) -> Option<Vec<Color>> {
    let (hostname, port) = get_shard_endpoint(routing_table, shard);
    let addr = format!("{}:{}", hostname, port);
    let mut stream = TcpStream::connect(&addr).ok()?;
    let req = BackendRequest::GetShardImage(GetShardImageRequest { shard });
    let encoded = bincode::serialize(&req).ok()?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).ok()?;
    stream.write_all(&encoded).ok()?;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).ok()?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).ok()?;
    let response: BackendResponse = bincode::deserialize(&buf).ok()?;
    if let BackendResponse::GetShardImage(GetShardImageResponse::Image { image }) = response {
        Some(image)
    } else {
        None
    }
}