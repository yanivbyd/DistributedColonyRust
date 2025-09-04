use crate::{colony::Colony, colony_shard::{WHITE_COLOR}};

use rand::{rngs::SmallRng, Rng};
use shared::{be_api::{Color, Shard, Traits}, log, utils::{random_chance, random_color}};

const RANDOM_EVENT_CHANCE: u32 = 10;

pub struct Circle {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
}

pub struct Ellipse {
    pub x: i32,
    pub y: i32,
    pub radius_x: i32,
    pub radius_y: i32,
}

pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub enum Region {
    Circle(Circle),
    Ellipse(Ellipse),
    Rectangle(Rectangle),
}

pub struct CreateCreatureParams {
    pub color: Color,
    pub traits: Traits,
    pub starting_health: u8,
}

pub struct RandomTraitParams {
    pub size: u8,
}

pub enum ColonyEvent {
    LocalDeath,
    RandomTrait(RandomTraitParams),
    CreateCreature(CreateCreatureParams),
}

fn point_inside_region(x: i32, y: i32, region: &Region) -> bool {
    match region {
        Region::Circle(circle) => {
            let dx = x - circle.x;
            let dy = y - circle.y;
            // Use saturating operations to prevent overflow
            let dx2 = dx.saturating_mul(dx);
            let dy2 = dy.saturating_mul(dy);
            let radius2 = circle.radius.saturating_mul(circle.radius);
            dx2.saturating_add(dy2) <= radius2
        }
        Region::Ellipse(ellipse) => {
            let dx = x - ellipse.x;
            let dy = y - ellipse.y;
            // Use saturating operations to prevent overflow
            let dx2 = dx.saturating_mul(dx);
            let dy2 = dy.saturating_mul(dy);
            let rx2 = ellipse.radius_x.saturating_mul(ellipse.radius_x);
            let ry2 = ellipse.radius_y.saturating_mul(ellipse.radius_y);
            
            // Calculate left side: dx²*ry² + dy²*rx²
            let left_side = dx2.saturating_mul(ry2).saturating_add(dy2.saturating_mul(rx2));
            
            // Calculate right side: rx²*ry²
            let right_side = rx2.saturating_mul(ry2);
            
            left_side <= right_side
        }
        Region::Rectangle(rect) => {
            x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
        }
    }
}

fn region_overlaps_shard(region: &Region, shard: &Shard) -> bool {
    let shard_right = shard.x + shard.width;
    let shard_bottom = shard.y + shard.height;
    
    match region {
        Region::Circle(circle) => {
            let closest_x = circle.x.max(shard.x).min(shard_right);
            let closest_y = circle.y.max(shard.y).min(shard_bottom);
            point_inside_region(closest_x, closest_y, region)
        },
        Region::Ellipse(ellipse) => {
            let closest_x = ellipse.x.max(shard.x).min(shard_right);
            let closest_y = ellipse.y.max(shard.y).min(shard_bottom);
            point_inside_region(closest_x, closest_y, region)
        },
        Region::Rectangle(rect) => {
            let rect_right = rect.x + rect.width;
            let rect_bottom = rect.y + rect.height;
            
            rect.x < shard_right && rect_right > shard.x && rect.y < shard_bottom && rect_bottom > shard.y
        }
    }
}

fn apply_region_to_shard<F>(
    shard: &mut crate::colony_shard::ColonyShard,
    region: &Region,
    mut cell_fn: F,
) 
where
    F: FnMut(&mut shared::be_api::Cell),
{
    let width = shard.shard.width as usize;
    let height = shard.shard.height as usize;
    let row_size = width + 2;

    for y in 0..height + 2 {
        for x in 0..width + 2 {
            let global_x = shard.shard.x + x as i32;
            let global_y = shard.shard.y + y as i32;

            if point_inside_region(global_x, global_y, region) {
                let idx = y * row_size + x;
                cell_fn(shard.grid.get_mut(idx).unwrap());
            }
        }
    }    
}

pub fn log_event(event: &ColonyEvent, region: &Region) {
    let event_details = match event {
        ColonyEvent::LocalDeath => "LocalDeath".to_string(),
        ColonyEvent::RandomTrait(params) => format!("RandomTrait, size {}", params.size),
        ColonyEvent::CreateCreature(params) => format!("CreateCreature, color {:?}, traits {:?}, health {}", 
            params.color, params.traits, params.starting_health),
    };
    
    let region_details = match region {
        Region::Circle(circle) => {
            format!("(Circle) at ({:.1}, {:.1}) with radius {:.1}", 
                circle.x, circle.y, circle.radius)
        },
        Region::Ellipse(ellipse) => {
            format!("(Ellipse) at ({:.1}, {:.1}) with radius ({:.1}, {:.1})", 
                ellipse.x, ellipse.y, ellipse.radius_x, ellipse.radius_y)
        },
        Region::Rectangle(rect) => {
            format!("(Rectangle) at ({:.1}, {:.1}) with size ({:.1}, {:.1})", 
                rect.x, rect.y, rect.width, rect.height)
        }
    };
    
    log!("Event: {} {}", event_details, region_details);
}

fn randomize_colony_event(rng: &mut SmallRng) -> ColonyEvent {
    match rng.gen_range(0..3) {
        0 => {
            ColonyEvent::LocalDeath
        },
        1 => {
            ColonyEvent::RandomTrait(RandomTraitParams {
                size: rng.gen_range(1..30),
            })
        },
        _ => {
            ColonyEvent::CreateCreature(CreateCreatureParams {
                color: random_color(rng),
                traits: Traits { size: rng.gen_range(1..30), can_kill: rng.gen_bool(0.5) },
                starting_health: 250,
            })
        }
    }
}

fn randomize_event_region(colony: &Colony, rng: &mut SmallRng) -> Region {
    match rng.gen_range(0..3) {
        0 => {
            Region::Circle(Circle {
                x: (rng.gen_range(0..colony._width + 200) - 100) as i32,
                y: (rng.gen_range(0..colony._height + 200) - 100) as i32,
                radius: rng.gen_range(5..30) as i32,
            })
        },
        1 => {
            Region::Ellipse(Ellipse {
                x: (rng.gen_range(0..colony._width + 200) - 100) as i32,
                y: (rng.gen_range(0..colony._height + 200) - 100) as i32,
                radius_x: rng.gen_range(15..40) as i32,
                radius_y: rng.gen_range(15..40) as i32,
            })
        },
        _ => {
            Region::Rectangle(Rectangle {
                x: (rng.gen_range(0..colony._width + 200) - 100) as i32,
                y: (rng.gen_range(0..colony._height + 200) - 100) as i32,
                width: rng.gen_range(5..40) as i32,
                height: rng.gen_range(5..40) as i32,
            })
        }
    }
}

pub fn randomize_event(colony: &Colony, rng: &mut SmallRng) -> Option<(ColonyEvent, Region)> {
    if random_chance(rng, RANDOM_EVENT_CHANCE) {
        return Some((randomize_colony_event(rng), randomize_event_region(colony, rng)));
    }    
    None
}

pub fn apply_event(colony: &mut Colony, event: &ColonyEvent, region: &Region) {
    for shard in &mut colony.shards {
        if !region_overlaps_shard(region, &shard.shard) {
            continue;
        }
        
        match event {
            ColonyEvent::LocalDeath => {
                apply_region_to_shard(shard, region, |cell| {
                    cell.color = WHITE_COLOR;
                    cell.health = 0;
                });
            },
            ColonyEvent::RandomTrait(params) => {
                apply_region_to_shard(shard, region, |cell| {
                    if cell.health > 0 {
                        cell.traits.size = params.size;
                    }
                });
            },
            ColonyEvent::CreateCreature(params) => {
                apply_region_to_shard(shard, region, |cell| {
                    cell.color = params.color;
                    cell.traits = params.traits;
                    cell.health = params.starting_health;
                });
            }
        }
    }
}
