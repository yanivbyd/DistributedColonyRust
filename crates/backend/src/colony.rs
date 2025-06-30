use shared::InitColonyRequest;

pub struct ColonySubGrid;

static COLONY_SUBGRID: ColonySubGrid = ColonySubGrid;

impl ColonySubGrid {
    pub fn instance() -> &'static ColonySubGrid {
        &COLONY_SUBGRID
    }

    pub fn init_colony(&self, _req: &InitColonyRequest) {
        // TODO: implement colony initialization
    }
} 