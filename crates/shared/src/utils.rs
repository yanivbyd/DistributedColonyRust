use rand::rngs::SmallRng;
use rand::{thread_rng, SeedableRng, Rng};

use crate::be_api::Color;

/// EC2 metadata service base URL
const EC2_METADATA_BASE: &str = "http://169.254.169.254/latest/meta-data";

/// Get EC2 instance private IP address
/// Returns None if not running on EC2 or if the request fails
pub async fn get_ec2_private_ip() -> Option<String> {
    let url = format!("{}/local-ipv4", EC2_METADATA_BASE);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    
    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                response.text().await.ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Get EC2 instance ID
/// Returns None if not running on EC2 or if the request fails
pub async fn get_ec2_instance_id() -> Option<String> {
    let url = format!("{}/instance-id", EC2_METADATA_BASE);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    
    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                response.text().await.ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Get EC2 instance public IP address
/// Returns None if not running on EC2 or if the request fails
pub async fn get_ec2_public_ip() -> Option<String> {
    let url = format!("{}/public-ipv4", EC2_METADATA_BASE);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .ok()?;
    
    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                response.text().await.ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Creates a new SmallRng instance seeded from the thread-local random number generator.
/// This provides a fast, non-cryptographic RNG that's suitable for most simulation purposes.
pub fn new_random_generator() -> SmallRng {
    SmallRng::from_rng(&mut thread_rng()).unwrap()
}

pub fn random_chance(rng: &mut SmallRng, out_of: u32) -> bool {
    rng.gen_range(1..=out_of) == 1
}

/// Generates a random color that is guaranteed to be visually distinct from white.
/// This ensures colors are bright and visible for creatures and other game elements.
pub fn random_color(rng: &mut SmallRng) -> crate::colony_model::Color {
    // Generate random RGB values, but ensure at least one channel is significantly low
    // to avoid colors that are too close to white (255, 255, 255)
    let mut red = rng.gen_range(0..=255);
    let mut green = rng.gen_range(0..=255);
    let mut blue = rng.gen_range(0..=255);
    
    let min_channel = red.min(green).min(blue);
    if min_channel > 240 {
        // Randomly choose one channel to reduce
        match rng.gen_range(0..3) {
            0 => red = rng.gen_range(0..240),
            1 => green = rng.gen_range(0..240),
            _ => blue = rng.gen_range(0..240),
        }
    }
    
    Color { red, green, blue }
}
