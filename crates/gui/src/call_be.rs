#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{BackendRequest, BackendResponse, GetShardImageRequest, GetShardImageResponse, GetShardLayerRequest, GetShardLayerResponse, ShardLayer, Shard, Color};
use shared::cluster_topology::{ClusterTopology, HostInfo};
use std::net::TcpStream;
use std::io::{Read, Write};
use bincode;

pub fn get_cluster_topology() -> &'static ClusterTopology {
    ClusterTopology::get_instance()
}

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
    let addr = host_info.to_address();
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

pub fn get_all_shard_layer_data(layer: ShardLayer, config: &crate::ShardConfig, topology: &ClusterTopology) -> Vec<Option<Vec<i32>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_layer_data(shard, layer, topology)).collect()
}

fn get_shard_layer_data(shard: Shard, layer: ShardLayer, topology: &ClusterTopology) -> Option<Vec<i32>> {
    let host_info = get_shard_endpoint(topology, shard);
    let addr = host_info.to_address();
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

pub fn get_all_shard_color_data(config: &crate::ShardConfig, topology: &ClusterTopology) -> Vec<Option<Vec<Color>>> {
    let shards: Vec<Shard> = (0..config.total_shards())
        .map(|i| config.get_shard(i))
        .collect();
    shards.iter().map(|&shard| get_shard_color_data(shard, topology)).collect()
}

fn get_shard_color_data(shard: Shard, topology: &ClusterTopology) -> Option<Vec<Color>> {
    let host_info = get_shard_endpoint(topology, shard);
    let addr = host_info.to_address();
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