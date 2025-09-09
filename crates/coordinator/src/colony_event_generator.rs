use shared::colony_events::{ColonyEvent, Region, Circle, Ellipse, CreateCreatureParams, RandomTraitParams};
use shared::be_api::Traits;
use shared::utils::random_color;
use rand::{rngs::SmallRng, Rng};

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum EventFrequency {
    Normal,
    Rare,
    Extinction,
}

pub fn randomize_colony_event(colony_width: i32, colony_height: i32, rng: &mut SmallRng) -> ColonyEvent {
    match rng.gen_range(0..3) {
        0 => {
            ColonyEvent::LocalDeath(randomize_event_region(colony_width, colony_height, rng))
        },
        1 => {
            ColonyEvent::RandomTrait(randomize_event_region(colony_width, colony_height, rng), RandomTraitParams {
                traits: Traits { size: rng.gen_range(1..30), can_kill: rng.gen_bool(0.5) },
            })
        },
        _ => {
            ColonyEvent::CreateCreature(randomize_event_region(colony_width, colony_height, rng), CreateCreatureParams {
                color: random_color(rng),
                traits: Traits { size: rng.gen_range(1..30), can_kill: rng.gen_bool(0.5) },
                starting_health: 250,
            })
        }
    }
}

pub fn randomize_event_region(colony_width: i32, colony_height: i32, rng: &mut SmallRng) -> Region {
    match rng.gen_range(0..2) {
        0 => {
            Region::Circle(Circle {
                x: (rng.gen_range(0..colony_width + 200) - 100) as i32,
                y: (rng.gen_range(0..colony_height + 200) - 100) as i32,
                radius: rng.gen_range(5..30) as i32,
            })
        },
        _ => {
            Region::Ellipse(Ellipse {
                x: (rng.gen_range(0..colony_width + 200) - 100) as i32,
                y: (rng.gen_range(0..colony_height + 200) - 100) as i32,
                radius_x: rng.gen_range(15..40) as i32,
                radius_y: rng.gen_range(15..40) as i32,
            })
        }
    }
}

pub fn randomize_event_by_frequency(frequency: EventFrequency, colony_width: i32, colony_height: i32, rng: &mut SmallRng) -> ColonyEvent {
    match frequency {
        EventFrequency::Normal => {
            randomize_colony_event(colony_width, colony_height, rng)
        },
        EventFrequency::Rare => {
            let sign: i8 = if rng.gen_bool(0.5) { 1 } else { -1 };
            let amount = sign * rng.gen_range(1..5);
            ColonyEvent::ChangeExtraFoodPerTick(amount)
        },
        EventFrequency::Extinction => {
            ColonyEvent::Extinction()
        }
    }
}

pub fn get_next_event_tick_by_frequency(frequency: EventFrequency, rng: &mut SmallRng) -> u64 {
    match frequency {
        EventFrequency::Normal => {
            rng.gen_range(5..20)
        },
        EventFrequency::Rare => {
            rng.gen_range(1000..5000)
        },
        EventFrequency::Extinction => {
            rng.gen_range(10000..50000)
        }
    }
}
