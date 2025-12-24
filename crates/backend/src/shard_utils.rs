use std::collections::BTreeMap;

use crate::colony_shard::{ColonyShard, is_blank};
use shared::{be_api::{Cell, ColonyLifeRules, Color, Shard, Traits, UpdatedShardContentsRequest, ShardLayer, StatMetric, ShardStatResult, StatBucket}};
use shared::log;
use rand::rngs::SmallRng;

pub struct ShardUtils;

impl ShardUtils {
    #[inline]
    fn accumulate_counts<F>(shard: &ColonyShard, mapper: F, include_blank_cells: bool) -> Vec<StatBucket>
    where
        F: Fn(&Cell) -> i32,
    {
        let width = shard.shard.width as usize;
        let height = shard.shard.height as usize;
        let row_size = width + 2;
        let mut counts: BTreeMap<i32, u64> = BTreeMap::new();
        for row_iter in 1..=height {
            let start = row_iter * row_size + 1;
            let end = start + width;
            for cell in &shard.grid[start..end] {
                if !include_blank_cells && cell.health == 0 { continue; }
                let value = mapper(cell);
                *counts.entry(value).or_insert(0) += 1;
            }
        }
        counts.into_iter().map(|(value, occs)| StatBucket { value, occs }).collect()
    }
    fn copy_cell_creature_data(dst: &mut Cell, src: &Cell, tick_bit: bool) {
        if dst.health > 0 && src.health == 0 { return; } // don't remove creatures from another shard
        dst.color = src.color;
        dst.original_color = src.original_color;
        dst.health = src.health;
        dst.age = src.age;
        dst.traits = src.traits;
        dst.food = src.food;
        dst.extra_food_per_tick = src.extra_food_per_tick;
        dst.tick_bit = tick_bit;        
    }

    pub fn compute_stats(shard: &ColonyShard, req_shard: &Shard, stats: &[StatMetric]) -> Option<Vec<ShardStatResult>> {
        if shard.shard.x != req_shard.x || shard.shard.y != req_shard.y || shard.shard.width != req_shard.width || shard.shard.height != req_shard.height {
            return None;
        }

        let mut metric_buckets: Vec<(StatMetric, Vec<StatBucket>)> = Vec::with_capacity(stats.len());
        for stat in stats.iter().copied() {
            let buckets = match stat {
                StatMetric::Health => Self::accumulate_counts(shard, |c| c.health as i32, false),
                StatMetric::Size => Self::accumulate_counts(shard, |c| c.traits.size as i32, false),
                StatMetric::CanKill => Self::accumulate_counts(shard, |c| if c.traits.can_kill { 1 } else { 0 }, false),
                StatMetric::CanMove => Self::accumulate_counts(shard, |c| if c.traits.can_move { 1 } else { 0 }, false),
                StatMetric::Food => Self::accumulate_counts(shard, |c| c.food as i32, true),
                StatMetric::Age => Self::accumulate_counts(shard, |c| c.age as i32, false),
            };
            metric_buckets.push((stat, buckets));
        }

        Some(vec![ShardStatResult { shard: shard.shard.clone(), metrics: metric_buckets }])
    }

    pub fn new_colony_shard(shard: &Shard, colony_life_rules: &ColonyLifeRules, rng: &mut SmallRng) -> ColonyShard {
        let white_color = Color { red: 255, green: 255, blue: 255 };
        let mut colony_shard = ColonyShard {
            shard: shard.clone(),
            colony_life_rules: colony_life_rules.clone(),
            current_tick: 0,
            grid: (0..((shard.width as usize + 2) * (shard.height as usize + 2))).map(|_| {
                Cell { 
                    color: white_color, 
                    original_color: white_color,
                    tick_bit: false, 
                    food: 50, 
                    extra_food_per_tick: 50,
                    health: 0,
                    age: 1,
                    traits: Traits { size: 1, can_kill: true, can_move: true },
                }
            }).collect(),
        };

        // State persistence removed - always start with randomized shard
        log!("Randomizing shard: {}", shard.to_id());
        colony_shard.randomize_at_start(rng);

        colony_shard
    }

    pub fn get_shard_image(shard: &ColonyShard, req_shard: &Shard) -> Option<Vec<Color>> {
        if shard.shard.x == req_shard.x && shard.shard.y == req_shard.y && shard.shard.width == req_shard.width && shard.shard.height == req_shard.height {
            let width = shard.shard.width as usize;
            let height = shard.shard.height as usize;
            let row_size = width + 2;
            let mut image = Vec::with_capacity(width * height);
            for row_iter in 1..=height {
                let start = row_iter * row_size + 1;
                let end = start + width;
                image.extend(shard.grid[start..end].iter().map(|cell| cell.color));
            }
            Some(image)
        } else {
            None
        }
    }

    pub fn get_shard_layer(shard: &ColonyShard, req_shard: &Shard, layer: &ShardLayer) -> Option<Vec<i32>> {
        if shard.shard.x == req_shard.x && shard.shard.y == req_shard.y && shard.shard.width == req_shard.width && shard.shard.height == req_shard.height {
            let width = shard.shard.width as usize;
            let height = shard.shard.height as usize;
            let row_size = width + 2;
            let mut data = Vec::with_capacity(width * height);
            for row_iter in 1..=height {
                let start = row_iter * row_size + 1;
                let end = start + width;
                match layer {
                    ShardLayer::CreatureSize => {
                        data.extend(shard.grid[start..end].iter().map(|cell| if is_blank(cell) { 0 } else { cell.traits.size as i32 }));
                    }
                    ShardLayer::Age => {
                        data.extend(shard.grid[start..end].iter().map(|cell| if is_blank(cell) { 0 } else { cell.age as i32 }));
                    }
                    ShardLayer::ExtraFood => {
                        data.extend(shard.grid[start..end].iter().map(|cell| cell.extra_food_per_tick as i32));
                    }
                    ShardLayer::CanKill => {
                        data.extend(shard.grid[start..end].iter().map(|cell| {
                            if is_blank(cell) {
                                0 // blank
                            } else if cell.traits.can_kill {
                                2 // can kill
                            } else {
                                1 // can't kill
                            }
                        }));
                    }
                    ShardLayer::CanMove => {
                        data.extend(shard.grid[start..end].iter().map(|cell| {
                            if is_blank(cell) {
                                0 // blank
                            } else if cell.traits.can_move {
                                2 // can move
                            } else {
                                1 // can't move
                            }
                        }));
                    }
                    ShardLayer::CostPerTurn => {
                        data.extend(shard.grid[start..end].iter().map(|cell| {
                            if is_blank(cell) {
                                0 // blank
                            } else {
                                ColonyShard::calculate_health_cost_for_cell(cell, &shard.colony_life_rules) as i32
                            }
                        }));
                    }
                    ShardLayer::Food => {
                        data.extend(shard.grid[start..end].iter().map(|cell| cell.food as i32));
                    }
                    ShardLayer::Health => {
                        data.extend(shard.grid[start..end].iter().map(|cell| cell.health as i32));
                    }
                }
            }
            Some(data)
        } else {
            None
        }
    }

    pub fn updated_shard_contents(my_shard: &mut ColonyShard, updated_shard_req: &UpdatedShardContentsRequest) {
        let my = &my_shard.shard;
        let other = &updated_shard_req.updated_shard;
        let width = my.width as usize;
        let height = my.height as usize;
        let row_size = width + 2;
        // Use a cell from the grid to get the current tick_bit value
        let tick_bit = my_shard.grid[width+4].tick_bit;

        // Check if the other shard is directly above
        if other.x == my.x && other.y + other.height == my.y && other.width == my.width {
            // Update top shadow lane (row 0, columns 1..=width) with the bottom border of the above shard
            let start = 1;
            for i in 0..width {
                let idx = start + i;
                Self::copy_cell_creature_data(&mut my_shard.grid[idx], &updated_shard_req.bottom[i], tick_bit);
            }
        }
        // Check if the other shard is directly below
        else if other.x == my.x && my.y + my.height == other.y && other.width == my.width {
            // Update bottom shadow lane (row height+1, columns 1..=width) with the top border of the below shard
            let start = (height + 1) * row_size + 1;
            for i in 0..width {
                let idx = start + i;
                Self::copy_cell_creature_data(&mut my_shard.grid[idx], &updated_shard_req.top[i], tick_bit);
            }
        }
        // Check if the other shard is directly to the left
        else if other.y == my.y && other.x + other.width == my.x && other.height == my.height {
            // Update left shadow lane (col 0, rows 1..=height) with the right border of the left shard
            for row in 1..=height {
                let idx = row * row_size;
                Self::copy_cell_creature_data(&mut my_shard.grid[idx], &updated_shard_req.right[row - 1], tick_bit);
            }
        }
        // Check if the other shard is directly to the right
        else if other.y == my.y && my.x + my.width == other.x && other.height == my.height {
            // Update right shadow lane (col width+1, rows 1..=height) with the left border of the right shard
            for row in 1..=height {
                let idx = row * row_size + (width + 1);
                Self::copy_cell_creature_data(&mut my_shard.grid[idx], &updated_shard_req.left[row - 1], tick_bit);
            }
        }
    }
    
    pub fn export_shard_contents(colony_shard: &ColonyShard) -> UpdatedShardContentsRequest {
        let shard = &colony_shard.shard;
        let width = shard.width as usize;
        let height = shard.height as usize;
        let row_size = width + 2;
        let grid = &colony_shard.grid;

        // Top border (row 1, columns 1..=width)
        let top = grid[1..=width].to_vec();
        // Bottom border (row height, columns 1..=width)
        let bottom_start = height * row_size + 1;
        let bottom_end = bottom_start + width;
        let bottom = grid[bottom_start..bottom_end].to_vec();
        // Left border (col 1, rows 1..=height)
        let mut left = Vec::with_capacity(height);
        for row in 1..=height {
            let idx = row * row_size + 1;
            left.push(grid[idx]);
        }
        // Right border (col width, rows 1..=height)
        let mut right = Vec::with_capacity(height);
        for row in 1..=height {
            let idx = row * row_size + width;
            right.push(grid[idx]);
        }

        UpdatedShardContentsRequest {
            updated_shard: shard.clone(),
            top,
            bottom,
            left,
            right,
        }
    }
    
    pub fn store_shard(_shard: &ColonyShard) {
        // State persistence removed - this method is now a no-op
        // Storage infrastructure remains for future high availability support
    }

    #[allow(dead_code)]
    fn get_shard_filename(shard: &Shard) -> String {
        format!("output/storage/{}.dat", shard.to_id())
    }

    #[allow(dead_code)]
    fn get_shard_temp_filename(shard: &Shard) -> String {
        format!("{}.tmp", Self::get_shard_filename(shard))
    }

    pub fn set_shadow_margin_tick_bits(colony_shard: &mut ColonyShard, tick_bit: bool) {
        let width = (colony_shard.shard.width + 2) as usize;
        let height = (colony_shard.shard.height + 2) as usize;        
        let bottom_start = (height - 1) * width;
        
        for x in 0..width {
            colony_shard.grid[x].tick_bit = tick_bit;
            colony_shard.grid[bottom_start + x].tick_bit = tick_bit;
        }
        
        for y in 1..height-1 {
            colony_shard.grid[y * width].tick_bit = tick_bit;
            colony_shard.grid[y * width + (width - 1)].tick_bit = tick_bit;
        }
    }

    pub fn count_tick_bits(colony_shard: &ColonyShard) -> (usize, usize) {
        let mut tick_bit_true_count = 0;
        let mut tick_bit_false_count = 0;
        
        for cell in &colony_shard.grid {
            if cell.tick_bit {
                tick_bit_true_count += 1;
            } else {
                tick_bit_false_count += 1;
            }
        }
        
        (tick_bit_true_count, tick_bit_false_count)
    }

    pub fn is_adjacent_shard(shard1: &Shard, shard2: &Shard) -> bool {
        // Check if shard2 is directly above shard1
        if shard2.x == shard1.x && shard2.y + shard2.height == shard1.y && shard2.width == shard1.width {
            return true;
        }
        // Check if shard2 is directly below shard1
        if shard2.x == shard1.x && shard1.y + shard1.height == shard2.y && shard2.width == shard1.width {
            return true;
        }
        // Check if shard2 is directly to the left of shard1
        if shard2.y == shard1.y && shard2.x + shard2.width == shard1.x && shard2.height == shard1.height {
            return true;
        }
        // Check if shard2 is directly to the right of shard1
        if shard2.y == shard1.y && shard1.x + shard1.width == shard2.x && shard2.height == shard1.height {
            return true;
        }
        false
    }

} 