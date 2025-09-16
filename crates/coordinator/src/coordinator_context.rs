use std::sync::{OnceLock, Mutex};
use crate::coordinator_storage::CoordinatorStoredInfo;
use shared::coordinator_api::ColonyEventDescription;

#[derive(Debug)]
pub struct CoordinatorContext {
    coord_stored_info: Mutex<CoordinatorStoredInfo>,
    colony_events: Mutex<Vec<ColonyEventDescription>>,
}

static COORDINATOR_CONTEXT: OnceLock<CoordinatorContext> = OnceLock::new();

impl CoordinatorContext {
    pub fn get_instance() -> &'static CoordinatorContext {
        COORDINATOR_CONTEXT.get_or_init(|| {
            CoordinatorContext {
                coord_stored_info: Mutex::new(CoordinatorStoredInfo::new()),
                colony_events: Mutex::new(Vec::new()),
            }
        })
    }

    pub fn initialize_with_stored_info(stored_info: CoordinatorStoredInfo) {
        COORDINATOR_CONTEXT.set(CoordinatorContext {
            coord_stored_info: Mutex::new(stored_info),
            colony_events: Mutex::new(Vec::new()),
        }).expect("CoordinatorContext should only be initialized once");
    }

    pub fn get_coord_stored_info(&self) -> std::sync::MutexGuard<CoordinatorStoredInfo> {
        self.coord_stored_info.lock().expect("Failed to acquire lock on coord_stored_info")
    }

    pub fn add_colony_event(&self, event: ColonyEventDescription) {
        let mut events = self.colony_events.lock().expect("Failed to acquire lock on colony_events");
        events.push(event);
    }

    pub fn get_colony_events(&self) -> std::sync::MutexGuard<Vec<ColonyEventDescription>> {
        self.colony_events.lock().expect("Failed to acquire lock on colony_events")
    }
}
