use serde::{Serialize, Deserialize};
use shared::{be_api::ColonyLifeRules, storage::StorageUtils};
use shared::coordinator_api::ColonyEventDescription;

#[allow(dead_code)]
pub const COORDINATOR_STATE_FILE: &str = "output/storage/colony.dat";

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
    pub colony_events: Vec<ColonyEventDescription>,
    pub pause_events_till: u64,
    pub cloud_start_idempotency_key: Option<String>,
}

impl CoordinatorStoredInfo {
    pub fn new() -> Self {
        Self {
            status: ColonyStatus::NotInitialized,
            colony_width: None,
            colony_height: None,
            colony_life_rules: None,
            colony_events: Vec::new(),
            pause_events_till: 0,
            cloud_start_idempotency_key: None,
        }
    }
    
    pub fn add_event(&mut self, event: ColonyEventDescription) {
        self.colony_events.push(event);
    }
    
    pub fn get_events(&self) -> &Vec<ColonyEventDescription> {
        &self.colony_events
    }
    
    pub fn set_pause_events_till(&mut self, tick: u64) {
        self.pause_events_till = tick;
    }
    
    pub fn is_events_paused(&self, current_tick: u64) -> bool {
        current_tick < self.pause_events_till
    }
    
    pub fn update_colony_rules(&mut self, new_rules: ColonyLifeRules) {
        self.colony_life_rules = Some(new_rules);
    }
}

#[allow(dead_code)]
pub struct CoordinatorStorage;

impl CoordinatorStorage {
    #[allow(dead_code)]
    pub fn store(info: &CoordinatorStoredInfo, filename: &str) -> Result<(), String> {
        StorageUtils::store_with_checksum(info, filename)
    }

    #[allow(dead_code)]
    pub fn retrieve(filename: &str) -> Option<CoordinatorStoredInfo> {
        StorageUtils::retrieve_with_checksum(filename)
    }
}