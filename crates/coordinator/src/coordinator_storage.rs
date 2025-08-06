use serde::{Serialize, Deserialize};
use shared::storage::StorageUtils;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ColonyStatus {
    NotInitialized,
    TopographyInitialized,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoordinatorInfo {
    pub status: ColonyStatus,
    pub colony_width: Option<i32>,
    pub colony_height: Option<i32>,
}

impl CoordinatorInfo {
    pub fn new() -> Self {
        Self {
            status: ColonyStatus::NotInitialized,
            colony_width: None,
            colony_height: None,
        }
    }
}

pub struct CoordinatorStorage;

impl CoordinatorStorage {
    pub fn store(info: &CoordinatorInfo, filename: &str) -> Result<(), String> {
        StorageUtils::store_with_checksum(info, filename)
    }

    pub fn retrieve(filename: &str) -> Option<CoordinatorInfo> {
        StorageUtils::retrieve_with_checksum(filename)
    }
}