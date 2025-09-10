// Global topography module for the coordinator
// This module will handle global topography-related functionality

use shared::be_api::{Shard, BackendRequest, BackendResponse, InitShardTopographyRequest, InitShardTopographyResponse};
use shared::{log, log_error};
use shared::utils::new_random_generator;
use shared::cluster_topology::ClusterTopology;
use shared::backend_communication::{send_request_async, receive_response_async};
use tokio::net::TcpStream;

#[derive(Debug)]
struct RiverPath {
    points: Vec<(f32, f32)>,
}

pub struct GlobalTopographyInfo {
    pub total_width: usize,
    pub total_height: usize,
    pub shard_width: usize,
    pub shard_height: usize,
    // River system parameters
    pub base_elevation: u8,
    pub river_elevation_range: u8, // How much elevation rivers add (base + range = max river elevation)
    pub river_influence_distance: f32, // Distance over which river influence extends
    pub river_count_range: (usize, usize), // (min, max) number of rivers
    pub river_segments_range: (usize, usize), // (min, max) segments per river
    pub river_step_length_range: (f32, f32), // (min, max) step length for river segments
    pub river_direction_change: f32, // Maximum direction change per segment
    pub smoothing_iterations: usize,
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

        let topology = ClusterTopology::get_instance();
        let host_info = match topology.get_host_for_shard(&shard) {
            Some(host) => host,
            None => {
                log_error!("Shard not found in cluster topology: ({},{},{},{})", 
                    shard.x, shard.y, shard.width, shard.height);
                return;
            }
        };
        
        if let Ok(mut stream) = TcpStream::connect(host_info.to_address()).await {
            if let Err(e) = send_request_async(&mut stream, &request).await {
                log_error!("Failed to send topography to shard ({},{},{},{}): {}", 
                    shard.x, shard.y, shard.width, shard.height, e);
                return;
            }

            match receive_response_async::<BackendResponse>(&mut stream).await {
                Ok(response) => {
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
                },
                Err(e) => {
                    log_error!("Failed to receive response for topography request: {}", e);
                }
            }
        } else {
            log_error!("Failed to connect to backend for topography request");
        }
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
        let mut image = vec![self.info.base_elevation; self.info.total_width * self.info.total_height];
        let mut rng = new_random_generator();
        
        // Step 1: Create river paths
        let river_paths = self.generate_river_paths(&mut rng);
        
        // Step 2: Apply river elevation and gradients
        for y in 0..self.info.total_height {
            for x in 0..self.info.total_width {
                let idx = y * self.info.total_width + x;
                let mut max_river_influence: f32 = 0.0;
                
                // Check distance to all river paths
                for river in &river_paths {
                    let distance = self.distance_to_river(x as f32, y as f32, river);
                    let influence = self.calculate_river_influence(distance);
                    max_river_influence = max_river_influence.max(influence);
                }
                
                // Apply river influence to elevation
                let river_elevation = (self.info.base_elevation as f32 + max_river_influence * self.info.river_elevation_range as f32) as u8;
                image[idx] = river_elevation;
            }
        }
        
        // Step 3: Apply gradient smoothing around rivers
        for _ in 0..self.info.smoothing_iterations {
            self.apply_laplacian_smoothing(&mut image);
        }
        
        image
    }


    
    fn generate_river_paths(&self, rng: &mut impl rand::Rng) -> Vec<RiverPath> {
        let mut rivers = Vec::new();
        let num_rivers = rng.gen_range(self.info.river_count_range.0..=self.info.river_count_range.1);
        
        for _ in 0..num_rivers {
            let river = self.generate_single_river(rng);
            rivers.push(river);
        }
        
        rivers
    }
    
    fn generate_single_river(&self, rng: &mut impl rand::Rng) -> RiverPath {
        let mut points = Vec::new();
        
        // Start from a random edge
        let start_side = rng.gen_range(0..4); // 0=top, 1=right, 2=bottom, 3=left
        let (start_x, start_y) = match start_side {
            0 => (rng.gen_range(0.0..self.info.total_width as f32), 0.0), // top
            1 => (self.info.total_width as f32, rng.gen_range(0.0..self.info.total_height as f32)), // right
            2 => (rng.gen_range(0.0..self.info.total_width as f32), self.info.total_height as f32), // bottom
            _ => (0.0, rng.gen_range(0.0..self.info.total_height as f32)), // left
        };
        
        points.push((start_x, start_y));
        
        // Generate river path with meandering
        let mut current_x = start_x;
        let mut current_y = start_y;
        let mut direction = rng.gen_range(0.0..std::f32::consts::PI * 2.0);
        
        let num_segments = rng.gen_range(self.info.river_segments_range.0..=self.info.river_segments_range.1);
        for _ in 0..num_segments {
            // Add some randomness to direction
            direction += rng.gen_range(-self.info.river_direction_change..self.info.river_direction_change);
            
            // Move in current direction
            let step_length = rng.gen_range(self.info.river_step_length_range.0..self.info.river_step_length_range.1);
            current_x += direction.cos() * step_length;
            current_y += direction.sin() * step_length;
            
            // Keep river within bounds
            current_x = current_x.clamp(0.0, self.info.total_width as f32);
            current_y = current_y.clamp(0.0, self.info.total_height as f32);
            
            points.push((current_x, current_y));
            
            // Stop if we've reached another edge
            if current_x <= 0.0 || current_x >= self.info.total_width as f32 ||
               current_y <= 0.0 || current_y >= self.info.total_height as f32 {
                break;
            }
        }
        
        RiverPath {
            points,
        }
    }
    
    fn distance_to_river(&self, x: f32, y: f32, river: &RiverPath) -> f32 {
        let mut min_distance = f32::INFINITY;
        
        // Check distance to each river segment
        for i in 0..river.points.len() - 1 {
            let (x1, y1) = river.points[i];
            let (x2, y2) = river.points[i + 1];
            
            let distance = self.distance_to_line_segment(x, y, x1, y1, x2, y2);
            min_distance = min_distance.min(distance);
        }
        
        min_distance
    }
    
    fn distance_to_line_segment(&self, px: f32, py: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
        let dx = x2 - x1;
        let dy = y2 - y1;
        
        if dx == 0.0 && dy == 0.0 {
            // Line segment is a point
            return ((px - x1).powi(2) + (py - y1).powi(2)).sqrt();
        }
        
        let t = ((px - x1) * dx + (py - y1) * dy) / (dx * dx + dy * dy);
        let t = t.clamp(0.0, 1.0);
        
        let closest_x = x1 + t * dx;
        let closest_y = y1 + t * dy;
        
        ((px - closest_x).powi(2) + (py - closest_y).powi(2)).sqrt()
    }
    
    fn calculate_river_influence(&self, distance: f32) -> f32 {
        // Create a smooth gradient that decreases with distance
        if distance <= self.info.river_influence_distance {
            // Smooth falloff using quadratic interpolation
            let t = distance / self.info.river_influence_distance;
            let influence = (1.0 - t).powi(2); // Quadratic falloff
            influence.clamp(0.0, 1.0)
        } else {
            0.0
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