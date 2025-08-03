use crate::colony_shard::ColonyShard;
use std::fs;
use std::path::Path;
use serde::{Serialize, Deserialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

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
        
        let mut data_with_checksum = serialized;
        data_with_checksum.extend_from_slice(&checksum.to_le_bytes());
        
        fs::write(filename, data_with_checksum)
            .map_err(|e| format!("Failed to write shard to file {}: {}", filename, e))
    }

}

