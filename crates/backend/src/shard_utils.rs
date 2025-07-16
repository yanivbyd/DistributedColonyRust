use crate::colony_shard::ColonyShard;
use crate::colony_shard::{Cell};
use shared::be_api::{Color, Shard};

pub struct ShardUtils;

impl ShardUtils {
    pub fn new_colony_shard(shard: Shard) -> ColonyShard {
        let white_color = Color { red: 255, green: 255, blue: 255 };
        let mut shard = ColonyShard {
            shard: Shard { x: 0, y: 0, width: shard.width, height: shard.height },
            grid: (0..(shard.width * shard.height)).map(|_| {
                Cell { color: white_color, tick_bit: false, strength: 0 }
            }).collect(),
        };
        shard.randomize_at_start();
        shard
    }

    pub fn get_shard_image(shard: &ColonyShard, req_shard: &Shard) -> Option<Vec<Color>> {
        if shard.shard.x == req_shard.x && shard.shard.y == req_shard.y && shard.shard.width == req_shard.width && shard.shard.height == req_shard.height {
            Some(shard.grid.iter().map(|cell| cell.color).collect())
        } else {
            None
        }
    }
} 