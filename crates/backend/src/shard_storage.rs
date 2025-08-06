use shared::be_api::Cell;
use crate::colony_shard::ColonyShard;
use shared::storage::StorageUtils;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct ShardStorage;

impl ShardStorage {
    pub fn store_shard(shard: &ColonyShard, filename: &str) -> Result<(), String> {
        StorageUtils::store_with_checksum(&shard.grid, filename)
    }

    pub fn retrieve_shard(shard: &mut ColonyShard, filename: &str) -> bool {
        if let Some(grid) = StorageUtils::retrieve_with_checksum::<Vec<Cell>>(filename) {
            shard.grid = grid;
            true
        } else {
            false
        }
    }
}

