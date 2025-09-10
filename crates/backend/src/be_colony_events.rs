use crate::{colony::Colony, colony_shard::WHITE_COLOR};

use rand::{rngs::SmallRng, Rng};
use shared::{be_api::Shard, colony_events::{ColonyEvent, Region}};

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



pub fn apply_event(rng: &mut SmallRng, colony: &Colony, event: &ColonyEvent) {
    match event {
        ColonyEvent::LocalDeath(region) => {
            apply_local_event(colony, event, region);
        },
        ColonyEvent::RandomTrait(region, _params) => {
            apply_local_event(colony, event, region);
        },
        ColonyEvent::CreateCreature(region, _params) => {
            apply_local_event(colony, event, region);
        }
        ColonyEvent::ChangeExtraFoodPerTick(amount) => {
            let (_, shard_arcs) = colony.get_hosted_shards();
            for shard_arc in shard_arcs {
                let mut shard = shard_arc.lock().unwrap();
                shard.grid.iter_mut().for_each(|cell| {
                    if *amount >= 0 {
                        cell.extra_food_per_tick = cell.extra_food_per_tick.saturating_add(*amount as u8);
                    } else {
                        cell.extra_food_per_tick = cell.extra_food_per_tick.saturating_sub((-*amount) as u8);
                    }
                });
            }            
        },
        ColonyEvent::Extinction() => {
            let (_, shard_arcs) = colony.get_hosted_shards();
            for shard_arc in shard_arcs {
                if rng.gen_bool(0.5) {
                    let mut shard = shard_arc.lock().unwrap();
                    shard.grid.iter_mut().for_each(|cell| {
                        cell.color = WHITE_COLOR;
                        cell.health = 0;
                    });
                }
            }
        }
    } 
}

pub fn apply_local_event(colony: &Colony, event: &ColonyEvent, region: &Region) {
    let (_, shard_arcs) = colony.get_hosted_shards();
    for shard_arc in shard_arcs {
        let mut shard = shard_arc.lock().unwrap();
        if !region_overlaps_shard(region, &shard.shard) {
            continue;
        }
        
        match event {
            ColonyEvent::LocalDeath(_region) => {
                apply_region_to_shard(&mut shard, region, |cell| {
                    cell.color = WHITE_COLOR;
                    cell.health = 0;
                });
            },
            ColonyEvent::RandomTrait(_region, params) => {
                apply_region_to_shard(&mut shard, region, |cell| {
                    if cell.health > 0 {
                        cell.traits = params.traits.clone();
                    }
                });
            },
            ColonyEvent::CreateCreature(_region, params) => {
                apply_region_to_shard(&mut shard, region, |cell| {
                    cell.color = params.color;
                    cell.traits = params.traits;
                    cell.health = params.starting_health;
                });
            },
            _ => {
                panic!("should not be called");
            }
        }
    }
}
