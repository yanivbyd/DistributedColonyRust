use serde::{Serialize, Deserialize};
use shared::storage::StorageUtils;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ColonyStatus {
    NotInitialized,
    ColonyInitialized,
    ShardInitialized,
    TopographyInitialized,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoordinatorInfo {
    pub status: ColonyStatus
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