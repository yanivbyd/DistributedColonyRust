use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::Path;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use crate::log_error;

pub const CHECKSUM_SIZE: usize = 8;

pub struct StorageUtils;

impl StorageUtils {
    pub fn store_with_checksum<T: Serialize>(data: &T, filename: &str) -> Result<(), String> {
        // Ensure the directory exists
        if let Some(parent) = Path::new(filename).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
        }
        
        let serialized = bincode::serialize(data)
            .map_err(|e| format!("Failed to serialize data: {}", e))?;
        
        // Calculate checksum
        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        let checksum = hasher.finish();
        
        // Combine data and checksum
        let mut data_with_checksum = serialized;
        data_with_checksum.extend_from_slice(&checksum.to_le_bytes());
        
        fs::write(filename, data_with_checksum)
            .map_err(|e| format!("Failed to write data to file {}: {}", filename, e))
    }

    pub fn retrieve_with_checksum<T: DeserializeOwned>(filename: &str) -> Option<T> {
        if !Path::new(filename).exists() {
            return None;
        }
        
        let content = match fs::read(filename) {
            Ok(content) => content,
            Err(_) => return None,
        };
        
        if content.len() < CHECKSUM_SIZE {
            return None;
        }
        
        // Split data and checksum
        let data_len = content.len() - CHECKSUM_SIZE;
        let (data, checksum_bytes) = content.split_at(data_len);
        let stored_checksum = match checksum_bytes.try_into() {
            Ok(bytes) => u64::from_le_bytes(bytes),
            Err(_) => return None,
        };
        
        // Verify checksum
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        let calculated_checksum = hasher.finish();
        
        if stored_checksum != calculated_checksum {
            log_error!("Data checksum mismatch for {}. Expected: {}, Got: {}", filename, stored_checksum, calculated_checksum);
            return None;
        }
        
        match bincode::deserialize(data) {
            Ok(data) => Some(data),
            Err(_) => None,
        }
    }
} 