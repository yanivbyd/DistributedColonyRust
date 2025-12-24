use shared::colony_events::{ColonyEvent, Region, Ellipse, CreateCreatureParams, ColonyRuleChange};
use shared::be_api::Traits;
use shared::utils::random_color;
use shared::be_api::ColonyLifeRules;
use rand::{rngs::SmallRng, Rng};

use crate::coordinator_context::CoordinatorContext;

#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
pub enum EventFrequency {
    Normal,
    Rare,
    Extinction,
    Topography,
    ColonyRules,
}

pub fn randomize_colony_event(colony_width: i32, colony_height: i32, rng: &mut SmallRng) -> ColonyEvent {
    ColonyEvent::CreateCreature(randomize_event_region(colony_width, colony_height, rng), CreateCreatureParams {
        color: random_color(rng),
        traits: Traits { size: rng.gen_range(1..20), can_kill: rng.gen_bool(0.5), can_move: rng.gen_bool(0.5) },
        starting_health: 600,
    })
}

pub fn randomize_event_region(colony_width: i32, colony_height: i32, rng: &mut SmallRng) -> Region {
    Region::Ellipse(Ellipse {
        x: (rng.gen_range(0..colony_width + 200) - 100) as i32,
        y: (rng.gen_range(0..colony_height + 200) - 100) as i32,
        radius_x: rng.gen_range(15..40) as i32,
        radius_y: rng.gen_range(15..40) as i32,
    })
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
        },
        EventFrequency::Topography => {
            ColonyEvent::NewTopography()
        },
        EventFrequency::ColonyRules => {
            randomize_colony_rules_change(CoordinatorContext::get_instance().get_colony_life_rules(), rng)
        }
    }
}

pub fn get_next_event_tick_by_frequency(frequency: EventFrequency, rng: &mut SmallRng) -> u64 {
    match frequency {
        EventFrequency::Normal => {
            rng.gen_range(400..500)
        },
        EventFrequency::Rare => {
            rng.gen_range(1000..2000)
        },
        EventFrequency::Extinction => {
            rng.gen_range(10000..50000)
        },
        EventFrequency::Topography => {
            rng.gen_range(5000..8000) 
        },
        EventFrequency::ColonyRules => {
            rng.gen_range(2000..3000)
        }
    }
}

fn apply_random_change(value: u32, min_value: u32, rng: &mut SmallRng) -> (u32, u32) {
    let old_value = value;
    
    // Calculate max change (20% of current value, rounded up)
    let max_change = (value as f32 * 0.2).ceil() as u32;
    
    // Determine if we can decrease (only if current value > min_value)
    let can_decrease = value > min_value;
    
    // Randomize increase/decrease, but only decrease if possible
    let is_increase = if can_decrease {
        rng.gen_bool(0.5)
    } else {
        true // force increase if we can't decrease
    };
    
    // Randomize the change amount between 1 and max_change
    let change_amount = if max_change > 0 {
        rng.gen_range(1..=max_change)
    } else {
        1
    };
    
    let new_value = if is_increase {
        value.saturating_add(change_amount)
    } else {
        value.saturating_sub(change_amount)
    }.max(min_value);
    
    (old_value, new_value)
}

fn apply_change_and_update(field: &mut u32, min_value: u32, rng: &mut SmallRng) -> (u32, u32) {
    let (old, new) = apply_random_change(*field, min_value, rng);
    *field = new;
    (old, new)
}

pub fn randomize_colony_rules_change(current_rules: ColonyLifeRules, rng: &mut SmallRng) -> ColonyEvent {
    const MIN_VALUE: u32 = 1;
    
    // Start with current rules passed as parameter
    let mut new_rules = current_rules;
    
    // Define the rule parameters that can be changed
    let rule_params = [
        "Health Cost Per Size Unit",
        "Eat Capacity Per Size Unit", 
        "Health Cost If Can Kill",
        "Health Cost If Can Move",
        "Mutation Chance",
        "Random Death Chance",
    ];
    
    // Randomly select which parameter to change
    let param_index = rng.gen_range(0..rule_params.len());
    let display_name = rule_params[param_index];
    
    // Apply the random change
    let (old_value, new_value) = match display_name {
        "Health Cost Per Size Unit" => apply_change_and_update(&mut new_rules.health_cost_per_size_unit, MIN_VALUE, rng),
        "Eat Capacity Per Size Unit" => apply_change_and_update(&mut new_rules.eat_capacity_per_size_unit, MIN_VALUE, rng),
        "Health Cost If Can Kill" => apply_change_and_update(&mut new_rules.health_cost_if_can_kill, MIN_VALUE, rng),
        "Health Cost If Can Move" => apply_change_and_update(&mut new_rules.health_cost_if_can_move, MIN_VALUE, rng),
        "Mutation Chance" => apply_change_and_update(&mut new_rules.mutation_chance, MIN_VALUE, rng),
        "Random Death Chance" => apply_change_and_update(&mut new_rules.random_death_chance, MIN_VALUE, rng),
        _ => panic!("Unknown parameter: {}", display_name),
    };
    
    let description = if new_value > old_value {
        format!("Increased '{}' from {} to {}", display_name, old_value, new_value)
    } else {
        format!("Decreased '{}' from {} to {}", display_name, old_value, new_value)
    };
    
    ColonyEvent::ChangeColonyRules(ColonyRuleChange {
        new_rules,
        description,
    })
}
