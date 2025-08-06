// Global topography module for the coordinator
// This module will handle global topography-related functionality

use shared::be_api::{Shard, BackendRequest, BackendResponse, InitShardTopographyRequest, InitShardTopographyResponse, BACKEND_PORT};
use shared::{log, log_error};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bincode;

pub struct GlobalTopographyInfo {
    pub total_width: usize,
    pub total_height: usize,
    pub shard_width: usize,
    pub shard_height: usize,
}

pub struct GlobalTopography {
    info: GlobalTopographyInfo,
}

impl GlobalTopography {
    pub fn new(info: GlobalTopographyInfo) -> Self {
        Self { info }
    }

    async fn send_topography_to_local_shard(&self, shard: Shard, topography_data: Vec<u8>) {
        let request = BackendRequest::InitShardTopography(InitShardTopographyRequest {
            shard,
            topography_data,
        });

        if let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{}", BACKEND_PORT)).await {
            if let Err(e) = Self::send_message(&mut stream, &request).await {
                log_error!("Failed to send topography to shard ({},{},{},{}): {}", 
                    shard.x, shard.y, shard.width, shard.height, e);
                return;
            }

            if let Some(response) = Self::receive_message::<BackendResponse>(&mut stream).await {
                match response {
                    BackendResponse::InitShardTopography(InitShardTopographyResponse::Ok) => {
                        log!("Topography sent to shard ({},{},{},{})", 
                            shard.x, shard.y, shard.width, shard.height);
                    },
                    BackendResponse::InitShardTopography(InitShardTopographyResponse::ShardNotInitialized) => {
                        log_error!("Shard not initialized for topography: ({},{},{},{})", 
                            shard.x, shard.y, shard.width, shard.height);
                    },
                    BackendResponse::InitShardTopography(InitShardTopographyResponse::InvalidTopographyData) => {
                        log_error!("Invalid topography data for shard: ({},{},{},{})", 
                            shard.x, shard.y, shard.width, shard.height);
                    },
                    _ => {
                        log_error!("Unexpected response for topography request");
                    }
                }
            } else {
                log_error!("Failed to receive response for topography request");
            }
        } else {
            log_error!("Failed to connect to backend for topography request");
        }
    }

    async fn send_message<T: serde::Serialize>(stream: &mut TcpStream, msg: &T) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = bincode::serialize(msg)?;
        let len = (encoded.len() as u32).to_be_bytes();
        stream.write_all(&len).await?;
        stream.write_all(&encoded).await?;
        Ok(())
    }

    async fn receive_message<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Option<T> {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            log_error!("Failed to read message length");
            return None;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        if stream.read_exact(&mut buf).await.is_err() {
            log_error!("Failed to read message body");
            return None;
        }
        bincode::deserialize(&buf).ok()
    }

    pub async fn generate_topography(&self) {
        log!("Generating global topography for colony {}x{}", self.info.total_width, self.info.total_height);
        
        // Create a full colony image with a global gradient
        let global_image = self.create_global_topography_image();
        
        // Calculate shard grid dimensions
        let horizontal_count = self.info.total_width / self.info.shard_width;
        let vertical_count = self.info.total_height / self.info.shard_height;
        
        log!("Distributing topography to {} shards ({}x{})", 
            horizontal_count * vertical_count, horizontal_count, vertical_count);

        // Send topography data to each shard
        for y in 0..vertical_count {
            for x in 0..horizontal_count {
                let shard = Shard {
                    x: (x * self.info.shard_width) as i32,
                    y: (y * self.info.shard_height) as i32,
                    width: self.info.shard_width as i32,
                    height: self.info.shard_height as i32,
                };

                // Extract shard-specific data from the global image
                let shard_data = self.extract_shard_data(&global_image, x, y);
                
                self.send_topography_to_local_shard(shard, shard_data).await;
            }
        }
        
        log!("Global topography generation completed");
    }

    fn create_global_topography_image(&self) -> Vec<u8> {
        let mut image = vec![0u8; self.info.total_width * self.info.total_height];
        
        // Create a radial gradient from the center of the colony
        let center_x = self.info.total_width as f32 / 2.0;
        let center_y = self.info.total_height as f32 / 2.0;
        let max_distance = ((center_x * center_x + center_y * center_y) as f32).sqrt();
        
        for y in 0..self.info.total_height {
            for x in 0..self.info.total_width {
                let idx = y * self.info.total_width + x;
                
                // Calculate distance from center
                let dx = x as f32 - center_x;
                let dy = y as f32 - center_y;
                let distance = (dx * dx + dy * dy).sqrt();
                
                // Create a radial gradient: higher values near center, lower at edges
                let normalized_distance = distance / max_distance;
                let gradient_value = ((1.0 - normalized_distance) * 255.0) as u8;
                
                // Add some variation with a simple noise pattern
                let noise = ((x as u64 * 73856093) ^ (y as u64 * 19349663)) % 50;
                let final_value = (gradient_value as u32 + noise as u32) as u8;
                
                image[idx] = final_value;
            }
        }
        
        image
    }

    fn extract_shard_data(&self, global_image: &[u8], shard_x: usize, shard_y: usize) -> Vec<u8> {
        let mut shard_data = Vec::with_capacity(self.info.shard_width * self.info.shard_height);
        
        let start_x = shard_x * self.info.shard_width;
        let start_y = shard_y * self.info.shard_height;
        
        for y in 0..self.info.shard_height {
            for x in 0..self.info.shard_width {
                let global_x = start_x + x;
                let global_y = start_y + y;
                let global_idx = global_y * self.info.total_width + global_x;
                
                if global_idx < global_image.len() {
                    let value = global_image[global_idx];
                    shard_data.push(value);
                } else {
                    shard_data.push(0); // Fallback value
                }
            }
        }
        
        shard_data
    }
}  