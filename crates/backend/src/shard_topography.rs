use crate::{colony_shard::ColonyShard, shard_utils::ShardUtils};
use shared::log;
pub struct ShardTopography;

impl ShardTopography {
    pub fn init_shard_topography_from_data(shard: &mut ColonyShard, topography_data: &[u8]) -> Result<(), ()> {
        log!("Initializing shard topography from data for shard ({},{},{},{})", 
            shard.shard.x, shard.shard.y, shard.shard.width, shard.shard.height);
        
        let expected_size = (shard.shard.width * shard.shard.height) as usize;
        if topography_data.len() != expected_size {
            log!("Topography data size mismatch: expected {}, got {}", expected_size, topography_data.len());
            return Err(());
        }
        
        // Apply the topography data directly to the shard's interior cells (excluding shadow margins)
        let width = (shard.shard.width + 2) as usize;
        
        // Initialize all cells with default value (0)
        for idx in 0..shard.grid.len() {
            shard.grid[idx].food = 0;
            shard.grid[idx].extra_food_per_tick = 0;
        }
        
        // Apply topography data to interior cells (skip shadow margins)
        for y in 0..shard.shard.height as usize {
            for x in 0..shard.shard.width as usize {
                let data_idx = y * shard.shard.width as usize + x;
                let grid_idx = (y + 1) * width + (x + 1); // +1 for shadow margin
                
                if data_idx < topography_data.len() && grid_idx < shard.grid.len() {
                    let value = topography_data[data_idx];
                    shard.grid[grid_idx].food = value;
                    shard.grid[grid_idx].extra_food_per_tick = value;
                }
            }
        }
        
        ShardUtils::store_shard(shard);
        Ok(())
    }

}
