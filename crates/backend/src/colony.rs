use shared::{InitColonyRequest, Color, GetSubImageRequest};

#[allow(static_mut_refs)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Color>,
}

#[allow(static_mut_refs)]
static mut COLONY_SUBGRID: Option<ColonySubGrid> = None;

#[allow(static_mut_refs)]
impl ColonySubGrid {
    pub fn instance() -> &'static mut ColonySubGrid {
        unsafe {
            COLONY_SUBGRID.as_mut().expect("ColonySubGrid is not initialized!")
        }
    }

    pub fn init_colony(req: &InitColonyRequest) {
        unsafe {
            if COLONY_SUBGRID.is_some() {
                panic!("ColonySubGrid is already initialized!");
            }
            COLONY_SUBGRID = Some(ColonySubGrid {
                width: req.width,
                height: req.height,
                grid: vec![Color { red: 255, green: 255, blue: 255 }; (req.width * req.height) as usize],
            });
        }
    }

    pub fn get_sub_image(&self, req: &GetSubImageRequest) -> Vec<Color> {
        let mut result = Vec::new();
        for y in req.y..(req.y + req.height) {
            for x in req.x..(req.x + req.width) {
                if y >= 0 && y < self.height && x >= 0 && x < self.width {
                    let idx = y as usize * self.width as usize + x as usize;
                    result.push(self.grid[idx]);
                }
            }
        }
        result
    }
}
