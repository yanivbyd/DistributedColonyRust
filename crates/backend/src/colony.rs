use std::sync::{Arc, RwLock, Mutex};
use std::sync::OnceLock;
use std::collections::HashMap;
use shared::be_api::{InitColonyRequest, Shard};
use crate::colony_shard::ColonyShard;

#[derive(Debug)]
pub struct Colony {
    pub _width: i32,
    pub _height: i32,
    pub shards: RwLock<HashMap<Shard, Arc<Mutex<ColonyShard>>>>, // HashMap for easy lookup, Arc<Mutex> for parallelism
}

static COLONY: OnceLock<Colony> = OnceLock::new();

impl Colony {
    pub fn instance() -> &'static Colony {
        COLONY.get().expect("Colony not initialized")
    }

    pub fn is_initialized() -> bool {
        COLONY.get().is_some()
    }

    pub fn init(req: &InitColonyRequest) {
        if COLONY.get().is_some() { return; }
        let colony = Colony {
            _width: req.width,
            _height: req.height,
            shards: RwLock::new(HashMap::new())
        };
        COLONY.set(colony).expect("Failed to init Colony");
    }

    pub fn add_shard(&self, colony_shard: ColonyShard) -> bool {
        let mut w = self.shards.write().unwrap();
        if w.contains_key(&colony_shard.shard) { return false; }
        w.insert(colony_shard.shard, Arc::new(Mutex::new(colony_shard)));
        true
    }

    pub fn has_shard(&self, shard: Shard) -> bool {
        self.shards.read().unwrap().contains_key(&shard)
    }

    pub fn get_all_shards(&self) -> (Vec<Shard>, Vec<Arc<Mutex<ColonyShard>>>) {
        let shards = self.shards.read().unwrap();
        let keys: Vec<Shard> = shards.keys().cloned().collect();
        let values: Vec<Arc<Mutex<ColonyShard>>> = shards.values().cloned().collect();
        (keys, values)
    }

    pub fn get_colony_shard_arc(&self, shard: &Shard) -> Option<Arc<Mutex<ColonyShard>>> {
        let r = self.shards.read().unwrap();
        r.get(shard).cloned()
    }

    pub fn is_valid_shard_dimensions(&self, shard: &Shard) -> bool {
        shard.x >= 0 && shard.y >= 0 &&
        shard.width > 0 && shard.height > 0 &&
        shard.x + shard.width <= self._width &&
        shard.y + shard.height <= self._height
    }

}
