use shared::{InitColonyRequest, Color};

#[allow(dead_code, static_mut_refs)]
pub struct ColonySubGrid {
    pub width: i32,
    pub height: i32,
    pub grid: Vec<Vec<Color>>,
}

#[allow(static_mut_refs)]
static mut COLONY_SUBGRID: Option<ColonySubGrid> = None;

#[allow(dead_code, static_mut_refs)]
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
                grid: vec![vec![Color { red: 255, green: 255, blue: 255 }; req.width as usize]; req.height as usize],
            });
        }
    }
}
