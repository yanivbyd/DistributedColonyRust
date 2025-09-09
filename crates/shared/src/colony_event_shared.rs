use crate::colony_events::{ColonyEvent, Region};
use crate::log;


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

