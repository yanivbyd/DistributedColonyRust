use shared::cluster_topology::{ClusterTopology, HostInfo};
use shared::colony_model::{Shard, Color};
use shared::{log, log_error};
use shared::ssm;
use shared::cluster_registry::create_cluster_registry;
use std::time::Duration;
use std::path::Path;
use image::{ImageBuffer, Rgb, RgbImage};

const BUCKET_DIR: &str = "output/s3/distributed-colony/images_shots";

/// Main function to capture colony creature images and save to disk
pub async fn capture_colony() {
    log!("Starting creature image capture");
    
    // Get topology
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized, skipping image capture");
            return;
        }
    };
    
    // Get colony dimensions from topology
    let (colony_width, colony_height) = match get_colony_dimensions(&topology) {
        Some(dims) => dims,
        None => {
            log_error!("Could not determine colony dimensions, skipping image capture");
            return;
        }
    };
    
    // Get all shards
    let shards = topology.get_all_shards();
    if shards.is_empty() {
        log_error!("No shards in topology, skipping image capture");
        return;
    }
    
    // Get coordinator timestamp for filename
    let timestamp = get_coordinator_timestamp();
    
    // Collect shard images
    let mut shard_images: Vec<(Shard, Vec<Color>)> = Vec::new();
    
    for shard in &shards {
        match get_shard_creature_image_http(&topology, *shard).await {
            Some(colors) => {
                shard_images.push((*shard, colors));
            }
            None => {
                log_error!("Failed to retrieve image for shard {:?}", shard);
            }
        }
    }
    
    if shard_images.is_empty() {
        log_error!("No shard images collected, skipping image save");
        return;
    }
    
    log!("Collected {} shard images", shard_images.len());
    
    // Combine shard images into single image
    let combined_image = combine_shard_images(&shard_images, colony_width, colony_height);
    
    // Save image to disk
    if let Err(e) = save_image_to_disk(&combined_image, &timestamp) {
        log_error!("Failed to save image to disk: {}", e);
        return;
    }
    
    log!("Successfully saved creature image to: {}/{}", BUCKET_DIR, timestamp);
    
    // Also capture statistics snapshot
    crate::colony_stats::capture_colony_stats().await;
}

/// Get colony dimensions from topology
fn get_colony_dimensions(topology: &ClusterTopology) -> Option<(i32, i32)> {
    let shard_width = topology.get_shard_width_from_mapping();
    let shard_height = topology.get_shard_height_from_mapping();
    let width_in_shards = topology.calculate_width_in_shards();
    let height_in_shards = topology.calculate_height_in_shards();
    
    if shard_width <= 0 || shard_height <= 0 || width_in_shards <= 0 || height_in_shards <= 0 {
        return None;
    }
    
    let colony_width = width_in_shards * shard_width;
    let colony_height = height_in_shards * shard_height;
    
    Some((colony_width, colony_height))
}

/// Get coordinator wall clock timestamp for filename
fn get_coordinator_timestamp() -> String {
    let now = chrono::Local::now();
    now.format("%Y_%m_%d__%H_%M_%S").to_string()
}

/// Get shard creature image via HTTP API
async fn get_shard_creature_image_http(topology: &ClusterTopology, shard: Shard) -> Option<Vec<Color>> {
    let host_info = topology.get_host_for_shard(&shard)?;
    
    // Get backend HTTP port using SSM discovery (similar to GUI pattern)
    let http_port = get_backend_http_port(host_info).await?;
    
    let shard_id = shard.to_id();
    let url = format!("http://{}:{}/api/shard/{}/image", host_info.hostname, http_port, shard_id);
    let width = shard.width as usize;
    let height = shard.height as usize;
    
    // Make HTTP request using blocking client (wrapped in spawn_blocking to avoid blocking async runtime)
    let url_clone = url.clone();
    let rgb_bytes = match tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(1500))
            .build()
            .ok()?;
        
        let response = client.get(&url_clone).send().ok()?;
        
        if !response.status().is_success() {
            return None;
        }
        
        response.bytes().ok()
    }).await {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            log_error!("HTTP request to {} failed or returned non-success status", url);
            return None;
        }
        Err(e) => {
            log_error!("HTTP request to {} panicked: {}", url, e);
            return None;
        }
    };
    
    // Convert raw RGB bytes to Vec<Color>
    if rgb_bytes.len() != width * height * 3 {
        log_error!("Invalid image data size for shard {:?}: expected {}, got {}", 
                   shard, width * height * 3, rgb_bytes.len());
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
}

/// Get backend HTTP port using SSM discovery (similar to GUI pattern)
async fn get_backend_http_port(host_info: &HostInfo) -> Option<u16> {
    // Try to discover backend HTTP port using SSM
    // Try both localhost and aws modes (similar to GUI pattern)
    for mode in &["localhost", "aws"] {
        let _registry = create_cluster_registry(mode);
        let backend_addresses = ssm::discover_backends().await;
        
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

/// Combine shard images into a single colony image
fn combine_shard_images(shard_images: &[(Shard, Vec<Color>)], colony_width: i32, colony_height: i32) -> RgbImage {
    // Create combined image buffer (colony_width × colony_height)
    let mut combined = ImageBuffer::<Rgb<u8>, Vec<u8>>::new(colony_width as u32, colony_height as u32);
    
    // Fill with black initially
    for pixel in combined.pixels_mut() {
        *pixel = Rgb([0, 0, 0]);
    }
    
    // For each shard image, place pixels in correct position
    for (shard, colors) in shard_images {
        let shard_width = shard.width as usize;
        let shard_height = shard.height as usize;
        
        // Validate shard dimensions match expected color count
        let expected_colors = shard_width * shard_height;
        if colors.len() != expected_colors {
            log_error!("Shard {:?} has {} colors but expected {} ({}x{})", 
                      shard, colors.len(), expected_colors, shard_width, shard_height);
            continue;
        }
        
        // Calculate global position from shard coordinates
        let global_x = shard.x;
        let global_y = shard.y;
        
        // Place pixels in combined image
        for (i, color) in colors.iter().enumerate() {
            let local_x = i % shard_width;
            let local_y = i / shard_width;
            
            let pixel_x = (global_x + local_x as i32) as u32;
            let pixel_y = (global_y + local_y as i32) as u32;
            
            // Check bounds
            if pixel_x < colony_width as u32 && pixel_y < colony_height as u32 {
                combined.put_pixel(pixel_x, pixel_y, Rgb([color.red, color.green, color.blue]));
            }
        }
    }
    
    combined
}

/// Save PNG image to disk with bucket directory structure
fn save_image_to_disk(image: &RgbImage, timestamp: &str) -> Result<(), String> {
    // Create directory if it doesn't exist
    let dir_path = Path::new(BUCKET_DIR);
    if let Err(e) = std::fs::create_dir_all(dir_path) {
        return Err(format!("Failed to create directory {}: {}", BUCKET_DIR, e));
    }
    
    // Construct full file path
    let filename = format!("{}.png", timestamp);
    let file_path = dir_path.join(&filename);
    
    // Save image as PNG
    image.save(&file_path)
        .map_err(|e| format!("Failed to save PNG image to {}: {}", file_path.display(), e))?;
    
    Ok(())
}

