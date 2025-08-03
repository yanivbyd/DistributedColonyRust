use shared::be_api::Cell;
use crate::colony_shard::ColonyShard;
use std::fs;
use std::path::Path;
use serde::{Serialize, Deserialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use shared::{log_error};

const CHECKSUM_SIZE: usize = 8;

#[derive(Serialize, Deserialize)]
pub struct ShardStorage;

impl ShardStorage {
    pub fn store_shard(shard: &ColonyShard, filename: &str) -> Result<(), String> {
        // Ensure the directory exists
        if let Some(parent) = Path::new(filename).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
        
        let serialized = bincode::serialize(&shard.grid)
            .map_err(|e| format!("Failed to serialize shard grid: {}", e))?;
        
        // Calculate checksum
        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        let checksum = hasher.finish();
        
        // Combine data and checksum
        let mut data_with_checksum = serialized;
        data_with_checksum.extend_from_slice(&checksum.to_le_bytes());
        
        fs::write(filename, data_with_checksum)
            .map_err(|e| format!("Failed to write shard to file {}: {}", filename, e))
    }

    pub fn retrieve_shard(shard: &mut ColonyShard, filename: &str) -> bool {
        if !Path::new(filename).exists() {
            return false;
        }
        
        let content = match fs::read(filename) {
            Ok(content) => content,
            Err(_) => return false,
        };
        
        if content.len() < CHECKSUM_SIZE {
            return false;
        }
        
        // Split data and checksum
        let data_len = content.len() - CHECKSUM_SIZE;
        let (data, checksum_bytes) = content.split_at(data_len);
        let stored_checksum = match checksum_bytes.try_into() {
            Ok(bytes) => u64::from_le_bytes(bytes),
            Err(_) => return false,
        };
        
        // Verify checksum
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        let calculated_checksum = hasher.finish();
        
        if stored_checksum != calculated_checksum {
            log_error!("Shard checksum mismatch for {}. Expected: {}, Got: {}", filename, stored_checksum, calculated_checksum);
            return false;
        }
        
        let grid: Vec<Cell> = match bincode::deserialize(data) {
            Ok(grid) => grid,
            Err(_) => return false,
        };
        
        shard.grid = grid;
        true
    }
}

