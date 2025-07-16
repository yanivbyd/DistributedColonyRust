use crate::colony_shard::ColonyShard;
use crate::colony_shard::{Cell};
use shared::be_api::{Color, GetSubImageRequest, InitColonyRequest, Shard};

pub struct ShardUtils;

impl ShardUtils {
    pub fn new_colony_shard(req: &InitColonyRequest) -> ColonyShard {
        let white_color = Color { red: 255, green: 255, blue: 255 };
        let mut shard = ColonyShard {
            shard: Shard { x: 0, y: 0, width: req.width, height: req.height },
            grid: (0..(req.width * req.height)).map(|_| {
                Cell { color: white_color, tick_bit: false, strength: 0 }
            }).collect(),
        };
        shard.randomize_at_start();
        shard
    }

    pub fn get_sub_image(shard: &ColonyShard, req: &GetSubImageRequest) -> Vec<Color> {
        if !(0 <= req.x && 0 <= req.y && req.width > 0 && req.height > 0 && 
            req.x + req.width <= shard.shard.width && req.y + req.height <= shard.shard.height) {
            return Vec::new();
        }

        let expected_len = (req.width * req.height) as usize;
        let mut result = Vec::with_capacity(expected_len);

        for y in req.y..(req.y + req.height) {
            for x in req.x..(req.x + req.width) {
                let idx = y as usize * shard.shard.width as usize + x as usize;
                result.push(shard.grid[idx].color);
            }
        }
        result
    }
} 