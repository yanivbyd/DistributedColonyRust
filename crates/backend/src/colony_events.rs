use crate::{colony::Colony, colony_shard::{is_blank, WHITE_COLOR}};

use rand::{rngs::SmallRng, Rng, SeedableRng};
use shared::{be_api::Shard, log};

pub struct Circle {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

pub struct Ellipse {
    pub x: f32,
    pub y: f32,
    pub radius_x: f32,
    pub radius_y: f32,
}

pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub enum Region {
    Circle(Circle),
    Ellipse(Ellipse),
    Rectangle(Rectangle),
}

pub enum ColonyEvent {
    LocalDeath(Region),
    RandomTrait(Region, u8), // Region and random size
}

fn region_overlaps_shard(region: &Region, shard: &Shard) -> bool {
    match region {
        Region::Circle(circle) => {
            let closest_x = circle.x.max(shard.x as f32).min((shard.x + shard.width) as f32);
            let closest_y = circle.y.max(shard.y as f32).min((shard.y + shard.height) as f32);
            let dx = circle.x - closest_x;
            let dy = circle.y - closest_y;
            (dx * dx + dy * dy) <= (circle.radius * circle.radius)
        },
        Region::Ellipse(ellipse) => {
            let closest_x = ellipse.x.max(shard.x as f32).min((shard.x + shard.width) as f32);
            let closest_y = ellipse.y.max(shard.y as f32).min((shard.y + shard.height) as f32);
            let dx = ellipse.x - closest_x;
            let dy = ellipse.y - closest_y;
            (dx * dx) / (ellipse.radius_x * ellipse.radius_x) + (dy * dy) / (ellipse.radius_y * ellipse.radius_y) <= 1.0
        },
        Region::Rectangle(rect) => {
            // Check if rectangle overlaps with shard
            let shard_right = shard.x + shard.width;
            let shard_bottom = shard.y + shard.height;
            let rect_right = rect.x + rect.width;
            let rect_bottom = rect.y + rect.height;
            
            rect.x < shard_right as f32 && rect_right > shard.x as f32 &&
            rect.y < shard_bottom as f32 && rect_bottom > shard.y as f32
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
            let global_x = shard.shard.x as f32 + x as f32;
            let global_y = shard.shard.y as f32 + y as f32;

            let inside = match region {
                Region::Circle(circle) => {
                    let dx = global_x - circle.x;
                    let dy = global_y - circle.y;
                    dx * dx + dy * dy <= circle.radius * circle.radius
                }
                Region::Ellipse(ellipse) => {
                    let dx = global_x - ellipse.x;
                    let dy = global_y - ellipse.y;
                    (dx * dx) / (ellipse.radius_x * ellipse.radius_x)
                        + (dy * dy) / (ellipse.radius_y * ellipse.radius_y)
                        <= 1.0
                }
                Region::Rectangle(rect) => {
                    global_x >= rect.x && global_x < rect.x + rect.width
                        && global_y >= rect.y && global_y < rect.y + rect.height
                }
            };

            if inside {
                let idx = (y + 1) * row_size + (x + 1);
                if let Some(cell) = shard.grid.get_mut(idx) {
                    cell_fn(cell);
                }
            }
        }
    }    
}

pub fn log_event(event: &ColonyEvent) {
    match event {
        ColonyEvent::LocalDeath(region) => {
            match region {
                Region::Circle(circle) => {
                    log!("[BE] Event: LocalDeath (Circle) at ({:.1}, {:.1}) with radius {:.1}", 
                         circle.x, circle.y, circle.radius);
                },
                Region::Ellipse(ellipse) => {
                    log!("[BE] Event: LocalDeath (Ellipse) at ({:.1}, {:.1}) with radius ({:.1}, {:.1})", 
                         ellipse.x, ellipse.y, ellipse.radius_x, ellipse.radius_y);
                },
                Region::Rectangle(rect) => {
                    log!("[BE] Event: LocalDeath (Rectangle) at ({:.1}, {:.1}) with size ({:.1}, {:.1})", 
                         rect.x, rect.y, rect.width, rect.height);
                }
            }
        },
        ColonyEvent::RandomTrait(region, size) => {
            match region {
                Region::Circle(circle) => {
                    log!("[BE] Event: RandomTrait (Circle) at ({:.1}, {:.1}) with radius {:.1}, size {}", 
                         circle.x, circle.y, circle.radius, size);
                },
                Region::Ellipse(ellipse) => {
                    log!("[BE] Event: RandomTrait (Ellipse) at ({:.1}, {:.1}) with radius ({:.1}, {:.1}), size {}", 
                         ellipse.x, ellipse.y, ellipse.radius_x, ellipse.radius_y, size);
                },
                Region::Rectangle(rect) => {
                    log!("[BE] Event: RandomTrait (Rectangle) at ({:.1}, {:.1}) with size ({:.1}, {:.1}), trait size {}", 
                         rect.x, rect.y, rect.width, rect.height, size);
                }
            }
        }
    }
}

pub fn randomize_event(colony: &Colony) -> Option<ColonyEvent> {
    let mut rng = SmallRng::from_entropy();    
    if rand::random::<f32>() > 0.1 {
        return None;
    }
    
    // Randomize region type (Circle, Ellipse, or Rectangle)
    let region = match rand::random::<u8>() % 3 {
        0 => {
            // Circle
            let circle = Circle {
                x: (rand::random::<i32>().abs() % (colony._width + 200) - 100) as f32,
                y: (rand::random::<i32>().abs() % (colony._height + 200) - 100) as f32,
                radius: (rand::random::<i32>().abs() % 20) as f32,
            };
            Region::Circle(circle)
        },
        1 => {
            // Ellipse
            let ellipse = Ellipse {
                x: (rand::random::<i32>().abs() % (colony._width + 200) - 100) as f32,
                y: (rand::random::<i32>().abs() % (colony._height + 200) - 100) as f32,
                radius_x: (rand::random::<i32>().abs() % 100) as f32,
                radius_y: (rand::random::<i32>().abs() % 100) as f32,
            };
            Region::Ellipse(ellipse)
        },
        _ => {
            // Rectangle
            let rect = Rectangle {
                x: (rand::random::<i32>().abs() % (colony._width + 200) - 100) as f32,
                y: (rand::random::<i32>().abs() % (colony._height + 200) - 100) as f32,
                width: (rand::random::<i32>().abs() % 50 + 10) as f32,
                height: (rand::random::<i32>().abs() % 50 + 10) as f32,
            };
            Region::Rectangle(rect)
        }
    };
    
    // Randomize event type
    if rand::random::<bool>() {
        Some(ColonyEvent::LocalDeath(region))
    } else {
        let random_size = rng.gen_range(1..99);
        Some(ColonyEvent::RandomTrait(region, random_size))
    }
}

pub fn apply_event(colony: &mut Colony, event: &ColonyEvent) {
    for shard in &mut colony.shards {
        match event {
            ColonyEvent::LocalDeath(region) => {
                if region_overlaps_shard(region, &shard.shard) {
                    apply_region_to_shard(shard, region, |cell| {
                        cell.color = WHITE_COLOR;
                    });
                }
            },
            ColonyEvent::RandomTrait(region, new_size) => {
                if region_overlaps_shard(region, &shard.shard) {
                    log!("[BE] RandomTrait: Applying size {} to shard ({}, {}, {}, {})", 
                         new_size, shard.shard.x, shard.shard.y, shard.shard.width, shard.shard.height);
                    apply_region_to_shard(shard, region, |cell| {
                        if !is_blank(cell) {
                            cell.traits.size = *new_size;
                        }
                    });
                }
            }
        }
    }
}
