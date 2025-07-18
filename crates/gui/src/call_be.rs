#![allow(deprecated)]
use eframe::egui;
use egui_extras::RetainedImage;
use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, GetShardImageRequest, GetShardImageResponse, Shard, Color};
use std::net::TcpStream;
use std::io::{Read, Write};
use bincode;

pub fn get_all_shard_retained_images() -> Vec<Option<RetainedImage>> {
    let third = 750 / 3;
    let shards = [
        Shard { x: 0, y: 0, width: third, height: third }, // top-left
        Shard { x: third, y: 0, width: third, height: third }, // top-middle
        Shard { x: 2 * third, y: 0, width: 750 - 2 * third, height: third }, // top-right
        Shard { x: 0, y: third, width: third, height: third }, // mid-left
        Shard { x: third, y: third, width: third, height: third }, // center
        Shard { x: 2 * third, y: third, width: 750 - 2 * third, height: third }, // mid-right
        Shard { x: 0, y: 2 * third, width: third, height: 750 - 2 * third }, // bottom-left
        Shard { x: third, y: 2 * third, width: third, height: 750 - 2 * third }, // bottom-middle
        Shard { x: 2 * third, y: 2 * third, width: 750 - 2 * third, height: 750 - 2 * third }, // bottom-right
    ];
    shards.iter().map(|&shard| get_shard_retained_image(shard)).collect()
}

fn get_shard_retained_image(shard: Shard) -> Option<RetainedImage> {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
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