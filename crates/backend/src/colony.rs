use shared::{InitColonyRequest, Color, GetSubImageRequest};
use std::sync::{Mutex, OnceLock};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

#[derive(Debug)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Color>,
}

static COLONY_SUBGRID: OnceLock<Mutex<ColonySubGrid>> = OnceLock::new();

const NUM_RANDOM_COLORS: usize = 50;

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
            if rng.gen_bool(0.8) {
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
