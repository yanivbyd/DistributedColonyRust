use shared::{InitColonyRequest, Color, GetSubImageRequest};
use std::sync::{Mutex, OnceLock};

#[derive(Debug)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Color>,
}

static COLONY_SUBGRID: OnceLock<Mutex<ColonySubGrid>> = OnceLock::new();

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
        COLONY_SUBGRID.set(Mutex::new(ColonySubGrid {
            width: req.width,
            height: req.height,
            grid: vec![Color { red: 255, green: 255, blue: 255 }; (req.width * req.height) as usize],
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
