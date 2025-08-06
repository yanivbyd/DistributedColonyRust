use shared::be_api::{Cell, ColonyLifeInfo, Color, Shard, Traits};
use shared::log;
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use std::cmp::min;
use std::sync::OnceLock;
use crate::shard_utils::ShardUtils;


pub const WHITE_COLOR: Color = Color { red: 255, green: 255, blue: 255 };
const LOG_TICK_STATS: bool = false;

#[derive(Clone, Copy)]
pub struct CreatureTemplate {
    pub color: Color,
    pub size: u8,
}

#[derive(Debug)]
struct TickStats {
    #[allow(dead_code)]
    tick_bit: bool,
    tick_true: usize,
    tick_false: usize,
    deaths: usize,
    moves: usize,
    breeds: usize,
    kills: usize,
}

impl TickStats {
    fn new(tick_bit: bool) -> Self {
        Self {
            tick_bit,
            tick_true: 0,
            tick_false: 0,
            deaths: 0,
            moves: 0,
            breeds: 0,
            kills: 0,
        }
    }
}

pub const NEIGHBOR_OFFSETS: [(isize, isize); 8] = [
    (-1, -1), (0, -1), (1, -1),
    (-1,  0),          (1,  0),
    (-1,  1), (0,  1), (1,  1),
];

pub static NEIGHBOR_PERMUTATIONS: OnceLock<Vec<[ (isize, isize); 8 ]>> = OnceLock::new();

pub fn get_neighbor_permutations() -> &'static Vec<[ (isize, isize); 8 ]> {
    NEIGHBOR_PERMUTATIONS.get_or_init(|| {
        let mut rng = rand::thread_rng();
        let mut perms = Vec::with_capacity(100);
        for _ in 0..100 {
            let mut arr = NEIGHBOR_OFFSETS;
            arr.shuffle(&mut rng);
            perms.push(arr);
        }
        perms
    })
}

#[derive(Debug)]
pub struct ColonyShard {
    pub shard: Shard,
    #[allow(dead_code)]
    pub colony_life_info: ColonyLifeInfo,    
    pub grid: Vec<Cell>,
}

impl ColonyShard {
    fn get_neighbors(x: usize, y: usize, width: usize, height: usize, offsets: &[(isize, isize)], my_cell: usize, neighbors: &mut [usize]) -> usize {
        let mut count = 0;
        for (dx, dy) in offsets.iter() {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if in_grid_range(width, height, nx, ny) {
                let neighbour = ny as usize * width + nx as usize;
                if neighbour != my_cell {
                    neighbors[count] = neighbour;
                    count += 1;
                }
            }
        }
        count
    }

    fn eat_food(&mut self, cell_idx: usize) {
        let food_eaten = min(
            self.grid[cell_idx].food,
            self.grid[cell_idx].traits.size * self.colony_life_info.eat_capacity_per_size_unit,
        );
        let health_cost = self.grid[cell_idx].traits.size * self.colony_life_info.health_cost_per_size_unit;
        self.grid[cell_idx].health = self.grid[cell_idx].health.saturating_add(food_eaten).saturating_sub(health_cost);
        self.grid[cell_idx].food = self.grid[cell_idx].food.saturating_sub(food_eaten);
    }

    fn move_to_highest_food_neighbor(&mut self, my_cell: usize, neighbors: &[usize], neighbor_count: usize, next_bit: bool) -> bool {
        let mut best_neighbor = None;
        let mut highest_food = self.grid[my_cell].food;
        
        // Find the neighbor with the highest food
        for i in 0..neighbor_count {
            let neighbor = neighbors[i];
            if is_blank(&self.grid[neighbor]) && self.grid[neighbor].food > highest_food {
                highest_food = self.grid[neighbor].food;
                best_neighbor = Some(neighbor);
            }
        }
        
        if let Some(best_neighbor) = best_neighbor {
            self.grid[best_neighbor].color = self.grid[my_cell].color;
            self.grid[best_neighbor].health = self.grid[my_cell].health;
            self.grid[best_neighbor].traits = self.grid[my_cell].traits;
            self.grid[best_neighbor].tick_bit = next_bit;
            set_blank(&mut self.grid[my_cell]);
            return true;
        }
        
        false
    }

    pub fn randomize_at_start(&mut self) {
        let mut rng = SmallRng::from_entropy();
        const NUM_RANDOM_CREATURES: usize = 3;
        let creature_templates: Vec<CreatureTemplate> = (0..NUM_RANDOM_CREATURES)
            .map(|_| CreatureTemplate {
                color: Color {
                    red: rng.gen_range(0..=255),
                    green: rng.gen_range(0..=255),
                    blue: rng.gen_range(0..=255),
                },
                size: 18,
            })
            .collect();

        for id in 0..self.grid.len() {
            self.grid[id].food = 10;
            self.grid[id].extra_food_per_tick = 10;

            if rng.gen_bool(0.1) {
                // create creatures
                let template = creature_templates[rng.gen_range(0..creature_templates.len())];
                self.grid[id].color = template.color;
                self.grid[id].health = 20;
                self.grid[id].traits.size = template.size;
            }
        }
    }

    pub fn tick(&mut self) {
        if self.grid.is_empty() { return; }
        let mut rng = SmallRng::from_entropy();
        let width = (self.shard.width + 2) as usize;
        let height = (self.shard.height + 2) as usize;
        let tick_bit = self.grid[width+4].tick_bit;
        let next_bit = !tick_bit;
        let neighbor_perms = get_neighbor_permutations();
        let mut offsets = &neighbor_perms[rng.gen_range(0..neighbor_perms.len())];
        let mut neighbors = [0; 8];
        let mut stats = TickStats::new(tick_bit);
        
        ShardUtils::set_shadow_margin_tick_bits(self, tick_bit);

        for my_cell in 0..self.grid.len() {
            // Increment food
            self.grid[my_cell].food = self.grid[my_cell].food.saturating_add(self.grid[my_cell].extra_food_per_tick);

            // Handle tick bit
            if self.grid[my_cell].tick_bit == next_bit {
                continue;
            } else {
                self.grid[my_cell].tick_bit = next_bit;
            }
            
            if is_blank(&self.grid[my_cell]) {
                continue;
            }

            // Randomize neightbor offsets
            if my_cell % 50 == 0 {
                offsets = &neighbor_perms[rng.gen_range(0..neighbor_perms.len())];
            }
            let neighbor_count = Self::get_neighbors(my_cell % width, my_cell / width, width, height, offsets, my_cell, &mut neighbors);

            // EAT food
            self.eat_food(my_cell);
            if self.grid[my_cell].health == 0 {
                set_blank(&mut self.grid[my_cell]);
                stats.deaths += 1;
                continue;
            }

            if self.breed(my_cell, &neighbors, neighbor_count, next_bit) {
                stats.breeds += 1;
            }

            if self.kill_neighbour(my_cell, &neighbors, neighbor_count, next_bit) {
                stats.kills += 1;
            }

            if self.move_to_highest_food_neighbor(my_cell, &neighbors, neighbor_count, next_bit) {
                stats.moves += 1;
            }
        }
        
        if LOG_TICK_STATS {
            (stats.tick_true, stats.tick_false) = ShardUtils::count_tick_bits(self);
            log!("Shard_{}_{}_{}_{}: {:?}", 
                self.shard.x, self.shard.y, self.shard.width, self.shard.height, stats);
        }
    }
    
    fn breed(&mut self, my_cell: usize, neighbors: &[usize], neighbor_count: usize, next_bit: bool) -> bool {
        if self.grid[my_cell].health < 200 {
            return false;
        }
        
        for i in 0..neighbor_count {
            let neighbor = neighbors[i];
            if is_blank(&self.grid[neighbor]) {
                // Calculate half health for the new creature
                let half_health = self.grid[my_cell].health / 2;
                
                // Create new creature with half health
                self.grid[neighbor].color = self.grid[my_cell].color;
                self.grid[neighbor].health = half_health;
                self.grid[neighbor].traits = self.copy_traits_with_mutation(&self.grid[my_cell].traits);
                self.grid[neighbor].tick_bit = next_bit;
                
                // Reduce parent's health by half
                self.grid[my_cell].health = self.grid[my_cell].health.saturating_sub(half_health);
                
                return true;
            }
        }
        
        false
    }
    
    fn copy_traits_with_mutation(&self, parent_traits: &Traits) -> Traits {
        let mut rng = SmallRng::from_entropy();
        
        // 1% chance to mutate size
        if rng.gen_bool(0.01) {
            let size_change = if rng.gen_bool(0.5) { 1 } else { -1 };
            let new_size = parent_traits.size.saturating_add_signed(size_change);
            Traits { size: new_size }
        } else {
            *parent_traits
        }
    }
    
    fn kill_neighbour(&mut self, my_cell: usize, neighbors: &[usize], neighbor_count: usize, next_bit: bool) -> bool {
        let my_color = self.grid[my_cell].color;
        
        for i in 0..neighbor_count {
            let neighbor = neighbors[i];
            if !is_blank(&self.grid[neighbor]) && 
               !self.grid[neighbor].color.equals(&my_color) && 
               self.grid[neighbor].tick_bit != next_bit &&
               self.grid[neighbor].traits.size < self.grid[my_cell].traits.size {
                set_blank(&mut self.grid[neighbor]);
                
                let health_reduction = self.grid[my_cell].health / 10;
                self.grid[my_cell].health = self.grid[my_cell].health.saturating_sub(health_reduction);
                
                return true;
            }
        }        
        false
    }
        
}

pub fn is_blank(cell: &Cell) -> bool {
    cell.color.red == 255 && cell.color.green == 255 && cell.color.blue == 255
}

fn set_blank(cell: &mut Cell) {
    cell.color = WHITE_COLOR;
    cell.health = 0;
}

pub fn in_grid_range(width: usize, height: usize, x: isize, y: isize) -> bool {
    x >= 0 && x < width as isize && y >= 0 && y < height as isize
} 