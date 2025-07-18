use crate::colony_shard::ColonyShard;
use shared::be_api::{Color, Shard, Cell};

pub struct ShardUtils;

impl ShardUtils {
pub fn new_colony_shard(shard: &Shard) -> ColonyShard {
        let white_color = Color { red: 255, green: 255, blue: 255 };
        let mut shard = ColonyShard {
            shard: shard.clone(),
            grid: (0..((shard.width+2) * (shard.height+2))).map(|_| {
                Cell { color: white_color, tick_bit: false, strength: 0 }
            }).collect(),
        };
        shard.randomize_at_start();
        shard
    }

    pub fn get_shard_image(shard: &ColonyShard, req_shard: &Shard) -> Option<Vec<Color>> {
        if shard.shard.x == req_shard.x && shard.shard.y == req_shard.y && shard.shard.width == req_shard.width && shard.shard.height == req_shard.height {
            let width = shard.shard.width as usize;
            let height = shard.shard.height as usize;
            let row_size = width + 2;
            let mut image = Vec::with_capacity(width * height);
            for row_iter in 1..=height {
                let start = row_iter * row_size + 1;
                let end = start + width;
                image.extend(shard.grid[start..end].iter().map(|cell| cell.color));
            }
            Some(image)
        } else {
            None
        }
    }
} 