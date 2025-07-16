use shared::be_api::{Color, GetSubImageRequest, InitColonyRequest, Shard};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct Cell {
    pub color: Color,
    pub tick_bit: bool,
    pub strength: u8    
}

pub const NUM_RANDOM_COLORS: usize = 50;

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
    pub grid: Vec<Cell>,
}

impl ColonyShard {
    pub fn new(req: &InitColonyRequest) -> Self {
        let mut rng = SmallRng::from_entropy();
        // Generate 50 random colors
        let random_colors: Vec<Color> = (0..NUM_RANDOM_COLORS)
            .map(|_| Color {
                red: rng.gen_range(0..=255),
                green: rng.gen_range(0..=255),
                blue: rng.gen_range(0..=255),
            })
            .collect();
        let grid = (0..(req.width * req.height)).map(|_| {
            let color = if rng.gen_bool(0.99) {
                Color { red: 255, green: 255, blue: 255 }
            } else {
                random_colors[rng.gen_range(0..random_colors.len())]
            };
            Cell { color, tick_bit: false, strength: rng.gen_range(20..255) }
        }).collect();

        ColonyShard {
            shard: Shard {
                x: 0,
                y: 0,
                width: req.width,
                height: req.height,
            },
            grid,
        }
    }

    pub fn tick(&mut self) {
        if self.grid.is_empty() { return; }
        let mut rng = SmallRng::from_entropy();
        let width = self.shard.width as usize;
        let height = self.shard.height as usize;
        let tick_bit = self.grid[0].tick_bit;
        let next_bit = !tick_bit;
        let neighbor_perms = get_neighbor_permutations();
        let mut offsets = &neighbor_perms[rng.gen_range(0..neighbor_perms.len())];
        let mut color_changes = 0;
        for y in 0..height {
            for x in 0..width {
                if rng.gen_bool(0.6) {
                    continue;
                }
                let my_cell = y * width + x;
                if self.grid[my_cell].tick_bit != tick_bit {
                    continue;
                }
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
                        if self.grid[neighbour].color.is_white() && self.grid[neighbour].tick_bit == tick_bit {
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
                            if self.grid[my_cell].color.is_different(&self.grid[neighbour].color) {
                                self.grid[neighbour].color = self.grid[my_cell].color;
                                self.grid[neighbour].strength = self.grid[my_cell].strength;
                                self.grid[neighbour].tick_bit = next_bit;
                                is_done = true;
                                color_changes += 1;
                                break;
                            }
                        }
                    }
                }
                if is_done { continue; }
            }
        }
        if color_changes <= 0 {
            self.meta_changes();
        }
    }

    pub fn get_sub_image(&self, req: &GetSubImageRequest) -> Vec<Color> {
        if !(0 <= req.x && 0 <= req.y && req.width > 0 && req.height > 0 && 
            req.x + req.width <= self.shard.width && req.y + req.height <= self.shard.height) {
            return Vec::new();
        }
    
        let expected_len = (req.width * req.height) as usize;
        let mut result = Vec::with_capacity(expected_len);
    
        for y in req.y..(req.y + req.height) {
            for x in req.x..(req.x + req.width) {
                let idx = y as usize * self.shard.width as usize + x as usize;
                result.push(self.grid[idx].color);
            }
        }
        result
    }
        
    pub fn meta_changes(&mut self) {
        let mut rng = SmallRng::from_entropy();
        for cell in self.grid.iter_mut() {
            if rng.gen_bool(0.5) {
                cell.strength = rng.gen_range(20..255);
            }
        }
    }
}

pub fn in_grid_range(width: usize, height: usize, x: isize, y: isize) -> bool {
    x >= 0 && x < width as isize && y >= 0 && y < height as isize
} 