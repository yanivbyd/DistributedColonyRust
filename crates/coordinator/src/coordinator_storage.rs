use serde::{Serialize, Deserialize};
use shared::{be_api::ColonyLifeRules, storage::StorageUtils};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ColonyStatus {
    NotInitialized,
    TopographyInitialized,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CoordinatorStoredInfo {
    pub status: ColonyStatus,
    pub colony_width: Option<i32>,
    pub colony_height: Option<i32>,
    pub colony_life_rules: Option<ColonyLifeRules>,
}

impl CoordinatorStoredInfo {
    pub fn new() -> Self {
        Self {
            status: ColonyStatus::NotInitialized,
            colony_width: None,
            colony_height: None,
            colony_life_rules: None,
        }
    }
}

pub struct CoordinatorStorage;

impl CoordinatorStorage {
    pub fn store(info: &CoordinatorStoredInfo, filename: &str) -> Result<(), String> {
        StorageUtils::store_with_checksum(info, filename)
    }

    pub fn retrieve(filename: &str) -> Option<CoordinatorStoredInfo> {
        StorageUtils::retrieve_with_checksum(filename)
    }
}