use shared::be_api::{InitColonyRequest, Shard};
use shared::log;
use std::{collections::HashMap, sync::{Mutex, OnceLock}};
use crate::colony_shard::ColonyShard;
use std::collections::hash_map::Entry;

#[derive(Debug)]
pub struct Colony {
    pub _width: i32,
    pub _height: i32,
    pub shards: HashMap<Shard, ColonyShard>,
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
            shards: HashMap::new(),
        })).expect("Failed to initialize ColonySubGrid");
    }

    pub fn has_shard(&self, shard: Shard) -> bool {
        self.shards.contains_key(&shard)
    }

    pub fn add_shard(&mut self, colony_shard: ColonyShard) -> bool {
        match self.shards.entry(colony_shard.shard.clone()) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                entry.insert(colony_shard);
                true
            }
        }
    }

    pub fn get_colony_shard(&self, shard: &Shard) -> Option<&ColonyShard> {
        self.shards.get(shard)
    }

    pub fn is_valid_shard_dimensions(&self, shard: &Shard) -> bool {
        shard.x >= 0 && shard.y >= 0 &&
        shard.width > 0 && shard.height > 0 &&
        shard.x + shard.width <= self._width &&
        shard.y + shard.height <= self._height
    }
}
