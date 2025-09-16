use std::sync::{OnceLock, Mutex};
use crate::coordinator_storage::CoordinatorStoredInfo;

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
}
