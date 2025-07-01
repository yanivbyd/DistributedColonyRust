use shared::be_api::{InitColonyRequest, Color, GetSubImageRequest};
use std::sync::{Mutex, OnceLock};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

#[derive(Debug, Clone)]
pub struct Cell {
    pub color: Color,
    pub tick_bit: bool,
}

#[derive(Debug)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Cell>,
}

static COLONY_SUBGRID: OnceLock<Mutex<ColonySubGrid>> = OnceLock::new();

const NUM_RANDOM_COLORS: usize = 50;

// Precompute all 24 permutations of NEIGHBOR_OFFSETS at compile time
const NEIGHBOR_PERMUTATIONS: [[(isize, isize); 4]; 24] = {
    let perms = [
        [(-1,0), (1,0), (0,-1), (0,1)],
        [(-1,0), (1,0), (0,1), (0,-1)],
        [(-1,0), (0,-1), (1,0), (0,1)],
        [(-1,0), (0,-1), (0,1), (1,0)],
        [(-1,0), (0,1), (1,0), (0,-1)],
        [(-1,0), (0,1), (0,-1), (1,0)],
        [ (1,0), (-1,0), (0,-1), (0,1)],
        [ (1,0), (-1,0), (0,1), (0,-1)],
        [ (1,0), (0,-1), (-1,0), (0,1)],
        [ (1,0), (0,-1), (0,1), (-1,0)],
        [ (1,0), (0,1), (-1,0), (0,-1)],
        [ (1,0), (0,1), (0,-1), (-1,0)],
        [ (0,-1), (-1,0), (1,0), (0,1)],
        [ (0,-1), (-1,0), (0,1), (1,0)],
        [ (0,-1), (1,0), (-1,0), (0,1)],
        [ (0,-1), (1,0), (0,1), (-1,0)],
        [ (0,-1), (0,1), (-1,0), (1,0)],
        [ (0,-1), (0,1), (1,0), (-1,0)],
        [ (0,1), (-1,0), (1,0), (0,-1)],
        [ (0,1), (-1,0), (0,-1), (1,0)],
        [ (0,1), (1,0), (-1,0), (0,-1)],
        [ (0,1), (1,0), (0,-1), (-1,0)],
        [ (0,1), (0,-1), (-1,0), (1,0)],
        [ (0,1), (0,-1), (1,0), (-1,0)],
    ];
    perms
};

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
            Cell { color, tick_bit: false }
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
        let mut offsets = &NEIGHBOR_PERMUTATIONS[rng.gen_range(0..NEIGHBOR_PERMUTATIONS.len())];
        for y in 0..height {
            for x in 0..width {
                if rng.gen_bool(0.8) {
                    // For performance reasons, do only 20% of the work
                    continue;
                }
                let idx = y * width + x;
                if self.grid[idx].tick_bit != tick_bit {
                    continue;
                }
                if idx % 50 == 0 {
                    offsets = &NEIGHBOR_PERMUTATIONS[rng.gen_range(0..NEIGHBOR_PERMUTATIONS.len())];
                }

                let my_color = self.grid[idx].color;
                for (dx, dy) in offsets.iter() {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && nx < width as isize && ny >= 0 && ny < height as isize {
                        let nidx = ny as usize * width + nx as usize;
                        let neighbor = &self.grid[nidx];
                        if neighbor.color.red == 255 && neighbor.color.green == 255 && neighbor.color.blue == 255 && neighbor.tick_bit == tick_bit {
                            self.grid[nidx].color = my_color;
                            self.grid[nidx].tick_bit = next_bit;
                            break;
                        }
                    }
                }
                self.grid[idx].tick_bit = next_bit;
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
