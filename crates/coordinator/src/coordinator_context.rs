use std::sync::{OnceLock, Mutex};
use crate::coordinator_storage::CoordinatorStoredInfo;
use shared::{coordinator_api::ColonyEventDescription, be_api::ColonyLifeRules};

#[derive(Debug)]
pub struct CoordinatorContext {
    coord_stored_info: Mutex<CoordinatorStoredInfo>,
}

static COORDINATOR_CONTEXT: OnceLock<CoordinatorContext> = OnceLock::new();

impl CoordinatorContext {
    pub fn get_instance() -> &'static CoordinatorContext {
        COORDINATOR_CONTEXT.get_or_init(|| {
            CoordinatorContext {
                coord_stored_info: Mutex::new(CoordinatorStoredInfo::new()),
            }
        })
    }

    pub fn initialize_with_stored_info(stored_info: CoordinatorStoredInfo) {
        COORDINATOR_CONTEXT.set(CoordinatorContext {
            coord_stored_info: Mutex::new(stored_info),
        }).expect("CoordinatorContext should only be initialized once");
    }

    pub fn get_coord_stored_info(&self) -> std::sync::MutexGuard<CoordinatorStoredInfo> {
        self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info")
    }

    pub fn add_colony_event(&self, event: ColonyEventDescription) {
        let mut stored_info = self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info");
        stored_info.add_event(event);
        drop(stored_info); // Release lock before calling storage
        
        // Store the updated info to disk
        let stored_info = self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info");
        if let Err(e) = crate::coordinator_storage::CoordinatorStorage::store(&stored_info, crate::coordinator_storage::COORDINATOR_STATE_FILE) {
            shared::log_error!("Failed to save coordination info: {}", e);
        }
    }

    pub fn get_colony_events(&self) -> Vec<ColonyEventDescription> {
        let stored_info = self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info");
        stored_info.get_events().clone()
    }
    
    pub fn get_colony_life_rules(&self) -> ColonyLifeRules {
        let stored_info = self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info");
        stored_info.colony_life_rules.unwrap_or_else(|| {
            // Fallback to initial rules if not set
            crate::init_colony::COLONY_LIFE_INITIAL_RULES
        })
    }
    
    pub fn update_colony_rules(&self, new_rules: ColonyLifeRules) {
        let mut stored_info = self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info");
        stored_info.update_colony_rules(new_rules);
        drop(stored_info); // Release lock before calling storage
        
        // Store the updated info to disk
        let stored_info = self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info");
        if let Err(e) = crate::coordinator_storage::CoordinatorStorage::store(&stored_info, crate::coordinator_storage::COORDINATOR_STATE_FILE) {
            shared::log_error!("Failed to save coordination info: {}", e);
        }
    }
}
