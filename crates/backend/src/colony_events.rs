use crate::colony::Colony;
use shared::log;

pub struct Circle {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
}

pub enum ColonyEvent {
    LocalDeath(Circle),
}

fn circle_overlaps_shard(circle: &Circle, shard: &shared::be_api::Shard) -> bool {
    // Find the closest point on the rectangle to the circle center
    let closest_x = circle.x.max(shard.x as f32).min((shard.x + shard.width) as f32);
    let closest_y = circle.y.max(shard.y as f32).min((shard.y + shard.height) as f32);
    let dx = circle.x - closest_x;
    let dy = circle.y - closest_y;
    (dx * dx + dy * dy) <= (circle.radius * circle.radius)
}

pub fn log_event(event: &ColonyEvent) {
    match event {
        ColonyEvent::LocalDeath(circle) => {
            log!("[BE] Event: LocalDeath at ({:.1}, {:.1}) with radius {:.1}", 
                 circle.x, circle.y, circle.radius);
        }
    }
}

pub fn randomize_event(colony: &Colony) -> Option<ColonyEvent> {
    if rand::random::<f32>() > 0.05 {
        return None;
    }
    let circle = Circle {
        x: rand::random::<f32>() * colony._width as f32,
        y: rand::random::<f32>() * colony._height as f32,
        radius: rand::random::<f32>() * 100.0, 
    };
    Some(ColonyEvent::LocalDeath(circle))
}

pub fn apply_event(colony: &mut Colony, event: &ColonyEvent) {
    for shard in &mut colony.shards {
        let circle = match event {
            ColonyEvent::LocalDeath(c) => c,
        };
        if circle_overlaps_shard(circle, &shard.shard) {
            match event {
                ColonyEvent::LocalDeath(circle) => {
                    let width = shard.shard.width as usize;
                    let height = shard.shard.height as usize;
                    let row_size = width + 2;
                    for y in 0..height {
                        for x in 0..width {
                            let global_x = shard.shard.x as f32 + x as f32;
                            let global_y = shard.shard.y as f32 + y as f32;
                            let dx = global_x - circle.x;
                            let dy = global_y - circle.y;
                            if dx * dx + dy * dy <= circle.radius * circle.radius {
                                let idx = (y + 1) * row_size + (x + 1);
                                if let Some(cell) = shard.grid.get_mut(idx) {
                                    cell.strength = rand::random::<u8>() % (cell.strength / 2 + 1); 
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
