use shared::be_api::{InitColonyRequest, Shard, ColonyLifeInfo};
use shared::{log, log_error};
use std::{sync::{Mutex, OnceLock}};
use crate::colony_shard::ColonyShard;

#[derive(Debug)]
pub struct Colony {
    pub _width: i32,
    pub _height: i32,
    pub shards: Vec<ColonyShard>,
    #[allow(dead_code)]
    pub colony_life_info: ColonyLifeInfo,
}

static COLONY: OnceLock<Mutex<Colony>> = OnceLock::new();

impl Colony {
    pub fn instance() -> std::sync::MutexGuard<'static, Colony> {
        match COLONY.get() {
            Some(colony) => colony.lock().expect("Failed to lock Colony"),
            None => {
                log_error!("Colony is not initialized! Attempting to access Colony before initialization.");
                eprintln!("{}", std::backtrace::Backtrace::capture());                
                panic!("Colony is not initialized! Make sure to call Colony::init() before accessing Colony::instance()");
            }
        }
    }

    pub fn is_initialized() -> bool {
        let initialized = COLONY.get().is_some();
        if !initialized {
            log_error!("Colony::is_initialized() called but Colony is not initialized");
        }
        initialized
    }

    pub fn init(req: &InitColonyRequest) {
        if COLONY.get().is_some() {
            log!("ColonySubGrid is already initialized!");
            return;
        }
        COLONY.set(Mutex::new(Colony {
            _width: req.width,
            _height: req.height,
            shards: Vec::new(),
            colony_life_info: req.colony_life_info.clone(),
        })).expect("Failed to initialize ColonySubGrid");
    }

    pub fn has_shard(&self, shard: Shard) -> bool {
        self.shards.iter().any(|colony_shard| colony_shard.shard == shard)
    }

    pub fn add_shard(&mut self, colony_shard: ColonyShard) -> bool {
        if self.has_shard(colony_shard.shard) {
            false
        } else {
            self.shards.push(colony_shard);
            true
        }
    }

    pub fn get_colony_shard(&self, shard: &Shard) -> Option<&ColonyShard> {
        self.shards.iter().find(|cs| &cs.shard == shard)
    }

    pub fn get_colony_shard_mut(&mut self, shard: &Shard) -> Option<&mut ColonyShard> {
        self.shards.iter_mut().find(|cs| &cs.shard == shard)
    }

    pub fn is_valid_shard_dimensions(&self, shard: &Shard) -> bool {
        shard.x >= 0 && shard.y >= 0 &&
        shard.width > 0 && shard.height > 0 &&
        shard.x + shard.width <= self._width &&
        shard.y + shard.height <= self._height
    }
}
