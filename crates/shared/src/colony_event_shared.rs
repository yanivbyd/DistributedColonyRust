use crate::colony_events::{ColonyEvent, Region};
use crate::log;
use crate::coordinator_api::ColonyEventDescription;


pub fn log_event(event: &ColonyEvent, current_tick: u64) {
    match event {
        ColonyEvent::LocalDeath(region) => {
            log_local_event(event, region, current_tick);
        },
        ColonyEvent::RandomTrait(region, _params) => {
            log_local_event(event, region, current_tick);
        },
        ColonyEvent::CreateCreature(region, _params) => {
            log_local_event(event, region, current_tick);
        },
        ColonyEvent::ChangeExtraFoodPerTick(amount) => {
            log!("[{}] Event: ChangeExtraFoodPerTick by {}", current_tick, amount);
        },
        ColonyEvent::Extinction() => {
            log!("[{}] Event: Extinction", current_tick);
        },
        ColonyEvent::NewTopography() => {
            log!("[{}] Event: NewTopography", current_tick);
        }
    }
}

pub fn log_local_event(event: &ColonyEvent, region: &Region, current_tick: u64) {
    let event_details = match event {
        ColonyEvent::LocalDeath(_region) => "LocalDeath".to_string(),
        ColonyEvent::RandomTrait(_region, params) => format!("RandomTrait, traits {:?}", 
            params.traits),
        ColonyEvent::CreateCreature(_region, params) => format!("CreateCreature, color {:?}, traits {:?}, health {}", 
            params.color, params.traits, params.starting_health),
        _ => {
            panic!("should not be called");
        }
    };
    
    let region_details = match region {
        Region::Circle(circle) => {
            format!("(Circle) at ({:.1}, {:.1}) with radius {:.1}", 
                circle.x, circle.y, circle.radius)
        },
        Region::Ellipse(ellipse) => {
            format!("(Ellipse) at ({:.1}, {:.1}) with radius ({:.1}, {:.1})", 
                ellipse.x, ellipse.y, ellipse.radius_x, ellipse.radius_y)
        }
    };
    
    log!("[{}] Event: {} {}", current_tick, event_details, region_details);
}

pub fn create_colony_event_description(event: &ColonyEvent, current_tick: u64) -> ColonyEventDescription {
    let (event_type, description) = match event {
        ColonyEvent::LocalDeath(region) => {
            ("Local Death".to_string(), format_local_event_description(event, region))
        },
        ColonyEvent::RandomTrait(region, _params) => {
            ("Random Trait".to_string(), format_local_event_description(event, region))
        },
        ColonyEvent::CreateCreature(region, _params) => {
            ("Create Creature".to_string(), format_local_event_description(event, region))
        },
        ColonyEvent::ChangeExtraFoodPerTick(amount) => {
            if *amount >= 0 {
                ("More Food".to_string(), format!("Extra food per tick by +{}", amount))
            } else {
                ("Less Food".to_string(), format!("Extra food per tick by {}", amount))
            }
        },
        ColonyEvent::Extinction() => {
            ("Extinction".to_string(), "Colony extinction event occurred".to_string())
        },
        ColonyEvent::NewTopography() => {
            ("New Topography".to_string(), "New topography generated".to_string())
        }
    };

    ColonyEventDescription {
        tick: current_tick,
        event_type,
        description,
    }
}

fn format_local_event_description(event: &ColonyEvent, region: &Region) -> String {
    let event_details = match event {
        ColonyEvent::LocalDeath(_region) => "LocalDeath".to_string(),
        ColonyEvent::RandomTrait(_region, params) => format!("RandomTrait, traits {:?}", 
            params.traits),
        ColonyEvent::CreateCreature(_region, params) => format!("CreateCreature, color {:?}, traits {:?}, health {}", 
            params.color, params.traits, params.starting_health),
        _ => {
            panic!("should not be called");
        }
    };
    
    let region_details = match region {
        Region::Circle(circle) => {
            format!("(Circle) at ({:.1}, {:.1}) with radius {:.1}", 
                circle.x, circle.y, circle.radius)
        },
        Region::Ellipse(ellipse) => {
            format!("(Ellipse) at ({:.1}, {:.1}) with radius ({:.1}, {:.1})", 
                ellipse.x, ellipse.y, ellipse.radius_x, ellipse.radius_y)
        }
    };
    
    format!("{} {}", event_details, region_details)
}

