use shared::be_api::{Color, Shard, Cell, ColonyLifeInfo};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use std::cmp::min;
use std::sync::OnceLock;
use crate::topography::Topography;


const WHITE_COLOR: Color = Color { red: 255, green: 255, blue: 255 };

#[derive(Clone, Copy)]
pub struct CreatureTemplate {
    pub color: Color,
    pub size: u8,
    pub strength: u8,
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
            self.grid[cell_idx].traits.size * self.colony_life_info.health_cost_per_size_unit,
        );
        let health_cost = self.grid[cell_idx].traits.size * self.colony_life_info.health_cost_per_size_unit;
        self.grid[cell_idx].health = self.grid[cell_idx].health.saturating_add(food_eaten).saturating_sub(health_cost);
        self.grid[cell_idx].food = self.grid[cell_idx].food.saturating_sub(food_eaten);
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
                size: 2,
                strength: 100,
            })
            .collect();

        for id in 0..self.grid.len() {
            if rng.gen_bool(0.99) {
                // create creatures
                let template = creature_templates[rng.gen_range(0..creature_templates.len())];
                self.grid[id].color = template.color;
                self.grid[id].strength = template.strength;
                self.grid[id].health = 20;
                self.grid[id].traits.size = template.size;
            }
        }

        Topography::init_topography(self);
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
        
        for my_cell in 0..self.grid.len() {
            // Increment food
            self.grid[my_cell].food = self.grid[my_cell].food.saturating_add(self.grid[my_cell].extra_food_per_tick);

            // Handle tick bit
            if self.grid[my_cell].tick_bit == next_bit || is_blank(&self.grid[my_cell]) {
                continue;
            } else {
                self.grid[my_cell].tick_bit = next_bit;
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
                continue;
            }

            // BREED to empty neighbouring cell
            let mut is_done = false;
            for i in 0..neighbor_count {
                let neighbour = neighbors[i];
                if is_blank(&self.grid[neighbour]) && self.grid[neighbour].tick_bit == tick_bit {
                    self.grid[neighbour].color = self.grid[my_cell].color;
                    self.grid[neighbour].strength = self.grid[my_cell].strength;
                    self.grid[neighbour].tick_bit = next_bit;
                    is_done = true;
                    break;
                }
            }
            if is_done { continue; }
            
            // KILL a neighbouring cell with lower strength
            for i in 0..neighbor_count {
                let neighbour = neighbors[i];
                if self.grid[my_cell].strength > self.grid[neighbour].strength {
                    self.grid[neighbour].color = self.grid[my_cell].color;
                    self.grid[neighbour].strength = self.grid[my_cell].strength-1;
                    self.grid[neighbour].tick_bit = next_bit;
                    is_done = true;
                    break;
                }
            }
            if is_done { continue; }
        }
    }
        
}

fn is_blank(cell: &Cell) -> bool {
    cell.color.red == 255 && cell.color.green == 255 && cell.color.blue == 255
}

fn set_blank(cell: &mut Cell) {
    cell.color = WHITE_COLOR;
    cell.strength = 0;
    cell.health = 0;
}

pub fn in_grid_range(width: usize, height: usize, x: isize, y: isize) -> bool {
    x >= 0 && x < width as isize && y >= 0 && y < height as isize
} 