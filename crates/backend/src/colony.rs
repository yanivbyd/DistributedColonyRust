use shared::be_api::InitColonyRequest;
use shared::log;
use std::sync::{Mutex, OnceLock};
use crate::colony_shard::ColonyShard;

#[derive(Debug)]
pub struct Colony {
    pub _width: i32,
    pub _height: i32,
    pub shard: Option<ColonyShard>,
}

static COLONY: OnceLock<Mutex<Colony>> = OnceLock::new();

impl Colony {
    pub fn instance() -> std::sync::MutexGuard<'static, Colony> {
        COLONY.get().expect("Colony is not initialized!").lock().expect("Failed to lock Colony")
    }

    pub fn is_initialized() -> bool {
        COLONY.get().is_some()
    }

    pub fn init(req: &InitColonyRequest) {
        if COLONY.get().is_some() {
            log!("ColonySubGrid is already initialized!");
            return;
        }
        COLONY.set(Mutex::new(Colony {
            _width: req.width,
            _height: req.height,
            shard: Some(ColonyShard::new(req))
        })).expect("Failed to initialize ColonySubGrid");
    }
}
