// Global topography module for the coordinator
// This module will handle global topography-related functionality

use shared::be_api::{Shard, BackendRequest, BackendResponse, InitShardTopographyRequest, InitShardTopographyResponse, BACKEND_PORT};
use shared::{log, log_error};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bincode;
use rand::Rng;

#[derive(Debug)]
struct SeedPoint {
    x: f32,
    y: f32,
    height: u8,
    radius: f32,
    influence_type: InfluenceType,
}

#[derive(Debug)]
enum InfluenceType {
    Peak,      // Creates high elevation
    Valley,    // Creates low elevation
    Ridge,     // Creates linear high elevation
    Plateau,   // Creates flat high elevation
    Random,    // Random influence pattern
}

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
        let mut rng = rand::thread_rng();
        
        // Step 1: Generate multiple seed points with different characteristics
        let seed_points = self.generate_seed_points(&mut rng);
        
        // Step 2: Create initial topography based on distance to seed points
        for y in 0..self.info.total_height {
            for x in 0..self.info.total_width {
                let idx = y * self.info.total_width + x;
                
                // Calculate influence from all seed points
                let mut total_influence = 0.0;
                let mut total_weight = 0.0;
                
                for (i, seed) in seed_points.iter().enumerate() {
                    let dx = x as f32 - seed.x;
                    let dy = y as f32 - seed.y;
                    let distance = (dx * dx + dy * dy).sqrt();
                    
                    // Calculate influence based on distance and seed characteristics
                    let influence = self.calculate_point_influence(distance, seed, i);
                    let weight = 1.0 / (1.0 + distance * 0.01); // Distance-based weight
                    
                    total_influence += influence * weight;
                    total_weight += weight;
                }
                
                // Normalize by total weight and add base gradient
                let base_value = if total_weight > 0.0 {
                    (total_influence / total_weight) as u8
                } else {
                    0
                };
                
                // Add structured randomness
                let noise1 = rng.gen_range(0..25); // Fine detail noise
                let noise2 = ((x as u64 * 73856093) ^ (y as u64 * 19349663)) % 35; // Coarse noise
                let noise3 = rng.gen_range(-15..15); // Medium detail noise
                
                let initial_value = (base_value as i32 + noise1 as i32 + noise2 as i32 + noise3) as u8;
                image[idx] = initial_value;
            }
        }
        
        // Step 3: Apply Laplacian smoothing multiple times
        let smoothing_iterations = 4;
        for _ in 0..smoothing_iterations {
            self.apply_laplacian_smoothing(&mut image);
        }
        
        // Step 4: Add final random variations
        for y in 0..self.info.total_height {
            for x in 0..self.info.total_height {
                let idx = y * self.info.total_width + x;
                let current_value = image[idx] as i32;
                
                // Add small random variations
                let variation = rng.gen_range(-8..8);
                let new_value = (current_value + variation).clamp(0, 255) as u8;
                image[idx] = new_value;
            }
        }
        
        image
    }

    fn generate_seed_points(&self, rng: &mut impl rand::Rng) -> Vec<SeedPoint> {
        let mut seeds = Vec::new();
        let num_seeds = rng.gen_range(5..12); // Random number of seed points
        
        // Always add a center seed point
        seeds.push(SeedPoint {
            x: self.info.total_width as f32 / 2.0,
            y: self.info.total_height as f32 / 2.0,
            height: rng.gen_range(180..220),
            radius: rng.gen_range(50.0..150.0),
            influence_type: InfluenceType::Peak,
        });
        
        // Generate additional random seed points
        for _ in 0..num_seeds {
            let x = rng.gen_range(0.0..self.info.total_width as f32);
            let y = rng.gen_range(0.0..self.info.total_height as f32);
            
            let influence_type = match rng.gen_range(0..5) {
                0 => InfluenceType::Peak,
                1 => InfluenceType::Valley,
                2 => InfluenceType::Ridge,
                3 => InfluenceType::Plateau,
                _ => InfluenceType::Random,
            };
            
            let height = match influence_type {
                InfluenceType::Peak => rng.gen_range(150..255),
                InfluenceType::Valley => rng.gen_range(0..100),
                InfluenceType::Ridge => rng.gen_range(120..200),
                InfluenceType::Plateau => rng.gen_range(100..180),
                InfluenceType::Random => rng.gen_range(50..200),
            };
            
            seeds.push(SeedPoint {
                x,
                y,
                height,
                radius: rng.gen_range(30.0..120.0),
                influence_type,
            });
        }
        
        seeds
    }

    fn calculate_point_influence(&self, distance: f32, seed: &SeedPoint, seed_index: usize) -> f32 {
        let normalized_distance = distance / seed.radius;
        
        match seed.influence_type {
            InfluenceType::Peak => {
                if normalized_distance <= 1.0 {
                    // Gaussian-like peak
                    let falloff = (-normalized_distance * normalized_distance * 2.0).exp();
                    seed.height as f32 * falloff
                } else {
                    0.0
                }
            },
            InfluenceType::Valley => {
                if normalized_distance <= 1.0 {
                    // Inverted peak for valley
                    let falloff = (-normalized_distance * normalized_distance * 1.5).exp();
                    -(seed.height as f32) * falloff
                } else {
                    0.0
                }
            },
            InfluenceType::Ridge => {
                if normalized_distance <= 1.0 {
                    // Linear ridge pattern
                    let ridge_factor = (seed_index as f32 * 0.5).sin() * 0.3 + 0.7;
                    let falloff = (-normalized_distance * normalized_distance * 1.0).exp();
                    seed.height as f32 * falloff * ridge_factor
                } else {
                    0.0
                }
            },
            InfluenceType::Plateau => {
                if normalized_distance <= 1.0 {
                    // Flat plateau with sharp edges
                    let plateau_factor = if normalized_distance < 0.8 { 1.0 } else { 0.0 };
                    seed.height as f32 * plateau_factor
                } else {
                    0.0
                }
            },
            InfluenceType::Random => {
                if normalized_distance <= 1.0 {
                    // Random pattern based on seed index
                    let random_factor = ((seed_index as f32 * 1.618033988749895) % 1.0).sin();
                    let falloff = (-normalized_distance * normalized_distance * 1.2).exp();
                    seed.height as f32 * falloff * random_factor
                } else {
                    0.0
                }
            },
        }
    }

    fn apply_laplacian_smoothing(&self, image: &mut [u8]) {
        let width = self.info.total_width;
        let height = self.info.total_height;
        let mut smoothed = vec![0u8; image.len()];
        
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let current = image[idx] as f32;
                
                // Collect neighbor values (with boundary checking)
                let mut neighbors = Vec::new();
                
                // Check all 8 neighbors
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 { continue; } // Skip current pixel
                        
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        
                        if nx >= 0 && nx < width as i32 && ny >= 0 && ny < height as i32 {
                            let neighbor_idx = (ny as usize) * width + (nx as usize);
                            neighbors.push(image[neighbor_idx] as f32);
                        }
                    }
                }
                
                if !neighbors.is_empty() {
                    // Laplacian smoothing: average with neighbors
                    let neighbor_avg = neighbors.iter().sum::<f32>() / neighbors.len() as f32;
                    
                    // Blend current value with neighbor average
                    // Using 0.7 weight for current value, 0.3 for neighbor average
                    let smoothed_value = current * 0.7 + neighbor_avg * 0.3;
                    smoothed[idx] = smoothed_value.round() as u8;
                } else {
                    smoothed[idx] = image[idx];
                }
            }
        }
        
        // Copy smoothed values back to original image
        image.copy_from_slice(&smoothed);
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