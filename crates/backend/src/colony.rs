use shared::be_api::{InitColonyRequest, Color, GetSubImageRequest};
use std::sync::{Mutex, OnceLock};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

#[derive(Debug, Clone)]
pub struct Cell {
    pub color: Color,
    pub tick_bit: bool,
    pub str: u8    
}

#[derive(Debug)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Cell>,
}

static COLONY_SUBGRID: OnceLock<Mutex<ColonySubGrid>> = OnceLock::new();

const NUM_RANDOM_COLORS: usize = 50;

// 8-neighbor offsets (including diagonals)
const NEIGHBOR_OFFSETS: [(isize, isize); 8] = [
    (-1, -1), (0, -1), (1, -1),
    (-1,  0),          (1,  0),
    (-1,  1), (0,  1), (1,  1),
];

// At startup, generate 100 random permutations of NEIGHBOR_OFFSETS
static NEIGHBOR_PERMUTATIONS: OnceLock<Vec<[ (isize, isize); 8 ]>> = OnceLock::new();

fn get_neighbor_permutations() -> &'static Vec<[ (isize, isize); 8 ]> {
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

fn in_grid_range(width: usize, height: usize, x: isize, y: isize) -> bool {
    x >= 0 && x < width as isize && y >= 0 && y < height as isize
}

impl ColonySubGrid {
    pub fn instance() -> std::sync::MutexGuard<'static, ColonySubGrid> {
        COLONY_SUBGRID
            .get()
            .expect("ColonySubGrid is not initialized!")
            .lock()
            .expect("Failed to lock ColonySubGrid")
    }

    pub fn init_colony(req: &InitColonyRequest) {
        if COLONY_SUBGRID.get().is_some() {
            panic!("ColonySubGrid is already initialized!");
        }
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
            Cell { color, tick_bit: false, str: rng.gen_range(20..255) }
        }).collect();
        COLONY_SUBGRID.set(Mutex::new(ColonySubGrid {
            width: req.width,
            height: req.height,
            grid,
        })).expect("Failed to initialize ColonySubGrid");
    }

    pub fn tick(&mut self) {
        if self.grid.is_empty() { return; }
        let mut rng = SmallRng::from_entropy();
        let width = self.width as usize;
        let height = self.height as usize;
        let tick_bit = self.grid[0].tick_bit;
        let next_bit = !tick_bit;
        let neighbor_perms = get_neighbor_permutations();
        let mut offsets = &neighbor_perms[rng.gen_range(0..neighbor_perms.len())];
        for y in 0..height {
            for x in 0..width {
                if rng.gen_bool(0.5) {
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
                            self.grid[neighbour].str = self.grid[my_cell].str;
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
                        if self.grid[my_cell].str > self.grid[neighbour].str {
                            if self.grid[my_cell].color.is_different(&self.grid[neighbour].color) {
                                self.grid[neighbour].color = self.grid[my_cell].color;
                                self.grid[neighbour].str = self.grid[my_cell].str;
                                self.grid[neighbour].tick_bit = next_bit;
                                is_done = true;
                                break;
                            }
                        }
                    }
                }
                if is_done { continue; }
            }
        }
    }

    pub fn get_sub_image(&self, req: &GetSubImageRequest) -> Vec<Color> {
        if !(0 <= req.x && 0 <= req.y && req.width > 0 && req.height > 0 && req.x + req.width <= self.width && req.y + req.height <= self.height) {
            return Vec::new();
        }
    
        let expected_len = (req.width * req.height) as usize;
        let mut result = Vec::with_capacity(expected_len);
    
        for y in req.y..(req.y + req.height) {
            for x in req.x..(req.x + req.width) {
                let idx = y as usize * self.width as usize + x as usize;
                result.push(self.grid[idx].color);
            }
        }
        result
    }
        
    pub fn is_initialized() -> bool {
        COLONY_SUBGRID.get().is_some()
    }
}
