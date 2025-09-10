use serde::{Deserialize, Serialize};
use shared::be_api::{Cell, ColonyLifeInfo, Color, Shard};
use shared::log;
use shared::utils::{new_random_generator, random_chance, random_color};
use rand::{Rng, rngs::SmallRng};
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
        let mut rng = new_random_generator();
        let mut perms = Vec::with_capacity(100);
        for _ in 0..100 {
            let mut arr = NEIGHBOR_OFFSETS;
            arr.shuffle(&mut rng);
            perms.push(arr);
        }
        perms
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ColonyShard {
    pub shard: Shard,
    pub colony_life_info: ColonyLifeInfo,    
    pub grid: Vec<Cell>,
    pub current_tick: u64, 
}

impl ColonyShard {
    pub fn get_current_tick(&self) -> u64 {
        self.current_tick
    }

    #[inline(always)]
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
        let food_eaten: u16 = min(
            self.grid[cell_idx].food,
            (self.grid[cell_idx].traits.size as u16).saturating_mul(self.colony_life_info.eat_capacity_per_size_unit as u16),
        );
        let health_cost: u16 = (self.grid[cell_idx].traits.size as u16).saturating_mul(self.colony_life_info.health_cost_per_size_unit as u16)
            + if self.grid[cell_idx].traits.can_kill { self.colony_life_info.health_cost_if_can_kill as u16 } else { 0 };
        self.grid[cell_idx].health = self.grid[cell_idx].health.saturating_add(food_eaten).saturating_sub(health_cost);
        self.grid[cell_idx].food = self.grid[cell_idx].food.saturating_sub(food_eaten);
    }

    #[inline(always)]
    fn move_to_higher_food_neighbor(&mut self, my_cell: usize, neighbors: &[usize], 
        neighbor_count: usize, next_bit: bool, rng: &mut SmallRng) -> bool 
    {
        let my_food = self.grid[my_cell].food.saturating_add(self.grid[my_cell].extra_food_per_tick as u16);

        for i in 0..neighbor_count {
            let n = neighbors[i];
            let n_food = self.grid[n].food.saturating_add(self.grid[n].extra_food_per_tick as u16);
            if self.grid[n].health == 0 && n_food > my_food {
                if random_chance(rng, 5) { return false; }

                self.grid[n].color = self.grid[my_cell].color;
                self.grid[n].health = self.grid[my_cell].health;
                self.grid[n].traits = self.grid[my_cell].traits;
                self.grid[n].tick_bit = next_bit;
                set_blank(&mut self.grid[my_cell]);
                return true;
            }
        }
        false
    }

    pub fn randomize_at_start(&mut self, rng: &mut SmallRng) {
        const NUM_RANDOM_CREATURES: usize = 3;
        let creature_templates: Vec<CreatureTemplate> = (0..NUM_RANDOM_CREATURES)
            .map(|_| CreatureTemplate {
                color: random_color(rng),
                size: 18,
            })
            .collect();

        for id in 0..self.grid.len() {
            if rng.gen_bool(0.1) {
                // create creatures
                let template = creature_templates[rng.gen_range(0..creature_templates.len())];
                self.grid[id].color = template.color;
                self.grid[id].health = 20;
                self.grid[id].traits.size = template.size;
            }
        }
    }

    #[inline(never)]
    pub fn tick(&mut self, rng: &mut SmallRng) {
        if self.grid.is_empty() { return; }        
        let width = (self.shard.width + 2) as usize;
        let height = (self.shard.height + 2) as usize;
        let tick_bit = self.grid[width+4].tick_bit;
        let next_bit = !tick_bit;
        let neighbor_perms = get_neighbor_permutations();
        let mut offsets: &[(isize, isize); 8];
        let mut neighbors = [0usize; 8];
        let mut stats = TickStats::new(tick_bit);        
        
        ShardUtils::set_shadow_margin_tick_bits(self, tick_bit);

        for y in 0..height {
            let row_base = y * width;
            for x in 0..width {
                let my_cell = row_base + x;

                // Increment food
                let cell = &mut self.grid[my_cell];
                cell.food = cell.food.saturating_add(cell.extra_food_per_tick as u16);

                // Handle tick bit
                if cell.tick_bit == next_bit {
                    continue;
                } 
                cell.tick_bit = next_bit;                
                
                if is_blank(cell) {
                    continue;
                }

                offsets = &neighbor_perms[rng.gen_range(0..neighbor_perms.len())];

                let neighbor_count = Self::get_neighbors(x, y, width, height, offsets, my_cell, &mut neighbors);

                self.eat_food(my_cell);
                if self.grid[my_cell].health == 0 || random_chance(rng, 5000) {
                    set_blank(&mut self.grid[my_cell]);
                    stats.deaths += 1;
                    continue;
                }

                if self.kill_neighbour(my_cell, &neighbors, neighbor_count, next_bit, rng) {
                    stats.kills += 1;
                } else {
                    if self.breed(my_cell, &neighbors, neighbor_count, next_bit, rng) {
                        stats.breeds += 1;
                    } else {
                        if self.move_to_higher_food_neighbor(my_cell, &neighbors, neighbor_count, next_bit, rng) {
                            stats.moves += 1;
                        }
                    }
                }
            }
        }
        
        if LOG_TICK_STATS {
            (stats.tick_true, stats.tick_false) = ShardUtils::count_tick_bits(self);
            log!("Shard_{}_{}_{}_{}: {:?}", 
                self.shard.x, self.shard.y, self.shard.width, self.shard.height, stats);
        }
        self.current_tick += 1;
    }
    
    fn breed(&mut self, my_cell: usize, neighbors: &[usize], neighbor_count: usize, next_bit: bool, rng: &mut SmallRng) -> bool {
        let cost_per_tick: u16 = (self.colony_life_info.health_cost_per_size_unit as u16).saturating_mul(self.grid[my_cell].traits.size as u16);
        if self.grid[my_cell].health <= cost_per_tick {
            return false;
        }
        
        for i in 0..neighbor_count {
            let neighbor = neighbors[i];
            if is_blank(&self.grid[neighbor]) {
                if random_chance(rng, 5) { return false; }  
                let half_health = self.grid[my_cell].health / 2;

                self.grid[neighbor].color = self.grid[my_cell].color;
                self.grid[neighbor].health = half_health;
                self.grid[neighbor].traits = self.grid[my_cell].traits;
                self.grid[neighbor].tick_bit = next_bit;
                if random_chance(rng, self.colony_life_info.mutation_chance) {
                    self.grid[neighbor] = self.mutate_cell(&self.grid[neighbor], rng);
                }
                self.grid[my_cell].health = self.grid[my_cell].health.saturating_sub(half_health);
                
                return true;
            }
        }
        
        false
    }
    
    fn mutate_cell(&self, cell: &Cell, rng: &mut SmallRng) -> Cell {        
        let mut new_cell = *cell;
        let size_change = if rng.gen_bool(0.5) { 1 } else { -1 };
        new_cell.traits.size = cell.traits.size.saturating_add_signed(size_change);
        new_cell.traits.can_kill = if random_chance(rng, 100) { cell.traits.can_kill } else { !cell.traits.can_kill };

        let color_mutation_range = 3; 
        let red_change = rng.gen_range(-color_mutation_range..=color_mutation_range);
        let green_change = rng.gen_range(-color_mutation_range..=color_mutation_range);
        let blue_change = rng.gen_range(-color_mutation_range..=color_mutation_range);
        
        new_cell.color.red = (cell.color.red as i16 + red_change).clamp(0, 255) as u8;
        new_cell.color.green = (cell.color.green as i16 + green_change).clamp(0, 255) as u8;
        new_cell.color.blue = (cell.color.blue as i16 + blue_change).clamp(0, 255) as u8;     
        new_cell   
    }
    
    #[inline(always)]
    fn kill_neighbour(&mut self, my_cell: usize, neighbors: &[usize], neighbor_count: usize, 
            next_bit: bool, rng: &mut SmallRng) -> bool 
    {
        if !self.grid[my_cell].traits.can_kill {
            return false;
        }
        let my_size  = self.grid[my_cell].traits.size;

        for i in 0..neighbor_count {
            let n = neighbors[i];
            let nref = &self.grid[n];
            if my_size > nref.traits.size && nref.health > 0 {
                if nref.traits.can_kill && !random_chance(rng, 10) { continue }
                self.grid[n].health = self.grid[my_cell].health.saturating_add(nref.health);
                self.grid[n].color = self.grid[my_cell].color;
                self.grid[n].traits = self.grid[my_cell].traits;
                self.grid[n].tick_bit = next_bit;

                set_blank(&mut self.grid[my_cell]);
                return true;
            }
        }
        false
    }
        
}

#[inline(always)]
pub fn is_blank(cell: &Cell) -> bool {
    cell.health == 0
}

#[cfg(debug_assertions)]
#[inline(always)]
fn assert_blank_consistency(cell: &Cell) {
    if cell.health == 0 {
        debug_assert!(
            cell.color.red == 255 && cell.color.green == 255 && cell.color.blue == 255,
            "blank sentinel mismatch: health==0 but color != WHITE"
        );
    }
}

#[inline(always)]
fn set_blank(cell: &mut Cell) {
    cell.color = WHITE_COLOR;
    cell.health = 0;
    #[cfg(debug_assertions)]
    assert_blank_consistency(cell);
}

#[inline(always)]
pub fn in_grid_range(width: usize, height: usize, x: isize, y: isize) -> bool {
    x >= 0 && x < width as isize && y >= 0 && y < height as isize
} 