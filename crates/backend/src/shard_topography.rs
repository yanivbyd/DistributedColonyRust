use crate::colony_shard::ColonyShard;
use shared::colony_model::ShardTopographyInfo;
use shared::log;
pub struct ShardTopography;

impl ShardTopography {
    pub fn init_shard_topography_from_info(shard: &mut ColonyShard, topography_info: &ShardTopographyInfo) {
        log!("Initializing shard topography from info for shard ({},{},{},{}), info={:?}", 
            shard.shard.x, shard.shard.y, shard.shard.width, shard.shard.height, 
            topography_info);
        let width = (shard.shard.width + 2) as usize;
        let height = (shard.shard.height + 2) as usize;
        
        // Initialize all cells with default value
        for idx in 0..shard.grid.len() {
            shard.grid[idx].food = topography_info.default_value;
            shard.grid[idx].extra_food_per_tick = topography_info.default_value;
        }

        // Apply border values
        Self::apply_borders(shard, topography_info, width, height);
        
        // Apply random points
        Self::apply_points(shard, topography_info, width, height);
        
        // Apply gradients to fill interior with smooth transitions
        Self::apply_gradients(shard, topography_info, width, height);
    }

    fn apply_borders(shard: &mut ColonyShard, topography_info: &ShardTopographyInfo, width: usize, height: usize) {
        // Apply top border (y = 1, inner top border)
        for x in 1..(shard.shard.width as usize + 1) {
            let idx = x; // y = 1, so just x
            if (x - 1) < topography_info.top_border.len() {
                shard.grid[idx].food = topography_info.top_border[x - 1];
                shard.grid[idx].extra_food_per_tick = topography_info.top_border[x - 1];
            }
        }

        // Apply bottom border (y = height - 2, inner bottom border)
        for x in 1..(shard.shard.width as usize + 1) {
            let idx = (height - 2) * width + x; // y = height - 2
            if (x - 1) < topography_info.bottom_border.len() {
                shard.grid[idx].food = topography_info.bottom_border[x - 1];
                shard.grid[idx].extra_food_per_tick = topography_info.bottom_border[x - 1];
            }
        }

        // Apply left border (x = 1, inner left border)
        for y in 1..(shard.shard.height as usize + 1) {
            let idx = y * width + 1; // x = 1
            if (y - 1) < topography_info.left_border.len() {
                shard.grid[idx].food = topography_info.left_border[y - 1];
                shard.grid[idx].extra_food_per_tick = topography_info.left_border[y - 1];
            }
        }

        // Apply right border (x = width - 2, inner right border)
        for y in 1..(shard.shard.height as usize + 1) {
            let idx = y * width + (width - 2); // x = width - 2
            if (y - 1) < topography_info.right_border.len() {
                shard.grid[idx].food = topography_info.right_border[y - 1];
                shard.grid[idx].extra_food_per_tick = topography_info.right_border[y - 1];
            }
        }
    }

    fn apply_points(shard: &mut ColonyShard, topography_info: &ShardTopographyInfo, width: usize, height: usize) {
        for &(x, y, value) in &topography_info.points {
            // Convert from shard-relative coordinates to grid coordinates (accounting for padding)
            let grid_x = x as usize + 1; // +1 for padding
            let grid_y = y as usize + 1; // +1 for padding
            
            if grid_x < width - 1 && grid_y < height - 1 { // Ensure we're not on borders
                let idx = grid_y * width + grid_x;
                if idx < shard.grid.len() {
                    shard.grid[idx].food = value;
                    shard.grid[idx].extra_food_per_tick = value;
                }
            }
        }
    }

    fn apply_gradients(shard: &mut ColonyShard, topography_info: &ShardTopographyInfo, width: usize, height: usize) {
        for y in 2..(height - 2) { // Skip border rows
            for x in 2..(width - 2) { // Skip border columns
                let idx = y * width + x;
                
                // Calculate distance to each border
                let dist_to_top = y - 1;
                let dist_to_bottom = height - 2 - y;
                let dist_to_left = x - 1;
                let dist_to_right = width - 2 - x;
                
                // Get border values (with bounds checking)
                let top_val = if x - 1 < topography_info.top_border.len() {
                    topography_info.top_border[x - 1]
                } else {
                    topography_info.default_value
                };
                
                let bottom_val = if x - 1 < topography_info.bottom_border.len() {
                    topography_info.bottom_border[x - 1]
                } else {
                    topography_info.default_value
                };
                
                let left_val = if y - 1 < topography_info.left_border.len() {
                    topography_info.left_border[y - 1]
                } else {
                    topography_info.default_value
                };
                
                let right_val = if y - 1 < topography_info.right_border.len() {
                    topography_info.right_border[y - 1]
                } else {
                    topography_info.default_value
                };
                
                // Calculate weighted average based on distance to borders
                let total_dist = dist_to_top + dist_to_bottom + dist_to_left + dist_to_right;
                let weighted_sum = (top_val as f32 * dist_to_bottom as f32) +
                                 (bottom_val as f32 * dist_to_top as f32) +
                                 (left_val as f32 * dist_to_right as f32) +
                                 (right_val as f32 * dist_to_left as f32);
                
                let interpolated_value = if total_dist > 0 {
                    (weighted_sum / total_dist as f32).round() as u8
                } else {
                    topography_info.default_value
                };
                
                // Apply gradient value
                shard.grid[idx].food = interpolated_value;
                shard.grid[idx].extra_food_per_tick = interpolated_value;
            }
        }
    }
}
