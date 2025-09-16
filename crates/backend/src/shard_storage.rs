use crate::{colony_shard::ColonyShard};
use shared::storage::StorageUtils;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct ShardStorage;

impl ShardStorage {
    pub fn store_shard(shard: &ColonyShard, filename: &str) -> Result<(), String> {
        StorageUtils::store_with_checksum(shard, filename)
    }

    pub fn retrieve_shard(shard: &mut ColonyShard, filename: &str) -> bool {
        if let Some(loaded_shard) = StorageUtils::retrieve_with_checksum::<ColonyShard>(filename) {
            shard.grid = loaded_shard.grid;
            shard.colony_life_rules = loaded_shard.colony_life_rules;
            shard.current_tick = loaded_shard.current_tick;
            assert_eq!(shard.shard, loaded_shard.shard);
            true
        } else {
            false
        }
    }
}

