use shared::be_api::{Color, Shard, Cell, ColonyLifeInfo};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use std::sync::OnceLock;
use crate::topography::Topography;

pub const NUM_RANDOM_COLORS: usize = 3;

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
    pub fn randomize_at_start(&mut self) {
        let mut rng = SmallRng::from_entropy();
        let random_colors: Vec<Color> = (0..NUM_RANDOM_COLORS)
            .map(|_| Color {
                red: rng.gen_range(0..=255),
                green: rng.gen_range(0..=255),
                blue: rng.gen_range(0..=255),
            })
            .collect();

        for id in 0..self.grid.len() {
            if rng.gen_bool(0.99) {
                // create creatures
                self.grid[id].color = random_colors[rng.gen_range(0..random_colors.len())];
                self.grid[id].strength = rng.gen_range(1..255);
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
        for my_cell in 0..self.grid.len() {
            if rng.gen_bool(0.6) {
                continue;
            }
            if self.grid[my_cell].tick_bit != tick_bit {
                continue;
            }
            let x = my_cell % width;
            let y = my_cell / width;
            self.grid[my_cell].tick_bit = next_bit;
            if my_cell % 50 == 0 {
                offsets = &neighbor_perms[rng.gen_range(0..neighbor_perms.len())];
            }
            let mut is_done = false;
            for (dx, dy) in offsets.iter() {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if in_grid_range(width, height, nx, ny) {
                    let neighbour = ny as usize * width + nx as usize;
                    if is_white(&self.grid[neighbour].color) && self.grid[neighbour].tick_bit == tick_bit {
                        self.grid[neighbour].color = self.grid[my_cell].color;
                        self.grid[neighbour].strength = self.grid[my_cell].strength;
                        self.grid[neighbour].tick_bit = next_bit;
                        is_done = true;
                        break;
                    }
                }
            }
            if is_done { continue; }
            for (dx, dy) in offsets.iter() {
                let nx = x as isize + dx;
                let ny = y as isize + dy;
                if in_grid_range(width, height, nx, ny) {
                    let neighbour = ny as usize * width + nx as usize;
                    if self.grid[my_cell].strength > self.grid[neighbour].strength {
                        self.grid[neighbour].color = self.grid[my_cell].color;
                        self.grid[neighbour].strength = self.grid[my_cell].strength-1;
                        self.grid[neighbour].tick_bit = next_bit;
                        is_done = true;
                        break;
                    }
                }
            }
            if is_done { continue; }
        }
    }
        
}

fn is_white(color: &Color) -> bool {
    color.red == 255 && color.green == 255 && color.blue == 255
}

pub fn in_grid_range(width: usize, height: usize, x: isize, y: isize) -> bool {
    x >= 0 && x < width as isize && y >= 0 && y < height as isize
} 