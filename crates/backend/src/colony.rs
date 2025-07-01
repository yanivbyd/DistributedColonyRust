use shared::{InitColonyRequest, Color, GetSubImageRequest};
use std::sync::{Mutex, OnceLock};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;

#[derive(Debug)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Color>,
}

static COLONY_SUBGRID: OnceLock<Mutex<ColonySubGrid>> = OnceLock::new();

const NUM_RANDOM_COLORS: usize = 50;
const NEIGHBOR_OFFSETS: [(isize, isize); 4] = [(-1,0), (1,0), (0,-1), (0,1)];

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
            if rng.gen_bool(0.99) {
                Color { red: 255, green: 255, blue: 255 }
            } else {
                random_colors[rng.gen_range(0..random_colors.len())]
            }
        }).collect();
        COLONY_SUBGRID.set(Mutex::new(ColonySubGrid {
            width: req.width,
            height: req.height,
            grid,
        })).expect("Failed to initialize ColonySubGrid");
        Self::instance().tick();
    }

    pub fn tick(&mut self) {
        let mut rng = SmallRng::from_entropy();
        let width = self.width as usize;
        let height = self.height as usize;
        let original_grid = self.grid.clone();
        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let my_color = original_grid[idx];
                // Shuffle neighbor offsets
                let mut offsets = NEIGHBOR_OFFSETS.to_vec();
                offsets.shuffle(&mut rng);
                for (dx, dy) in offsets.iter() {
                    let nx = x as isize + dx;
                    let ny = y as isize + dy;
                    if nx >= 0 && nx < width as isize && ny >= 0 && ny < height as isize {
                        let nidx = ny as usize * width + nx as usize;
                        let neighbor = original_grid[nidx];
                        if neighbor.red == 255 && neighbor.green == 255 && neighbor.blue == 255 {
                            self.grid[nidx] = my_color;
                            break;
                        }
                    }
                }
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
                result.push(self.grid[idx]);
            }
        }
        result
    }
        
}
