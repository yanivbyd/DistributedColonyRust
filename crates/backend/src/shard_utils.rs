use crate::colony_shard::ColonyShard;
use shared::be_api::{Cell, Color, Shard, UpdatedShardContentsRequest};

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

    pub fn updated_shard_contents(my_shard: &mut ColonyShard, updated_shard_req: &UpdatedShardContentsRequest) {
        let my = &my_shard.shard;
        let other = &updated_shard_req.updated_shard;
        let width = my.width as usize;
        let height = my.height as usize;
        let row_size = width + 2;

        // Check if the other shard is directly above
        if other.x == my.x && other.y + other.height == my.y && other.width == my.width {
            // Update top shadow lane (row 0, columns 1..=width)
            let start = 1;
            let end = start + width;
            my_shard.grid[start..end].clone_from_slice(&updated_shard_req.bottom);
        }
        // Check if the other shard is directly below
        else if other.x == my.x && my.y + my.height == other.y && other.width == my.width {
            // Update bottom shadow lane (row height+1, columns 1..=width)
            let start = (height + 1) * row_size + 1;
            let end = start + width;
            my_shard.grid[start..end].clone_from_slice(&updated_shard_req.top);
        }
        // Check if the other shard is directly to the left
        else if other.y == my.y && other.x + other.width == my.x && other.height == my.height {
            // Update left shadow lane (col 0, rows 1..=height)
            for row in 1..=height {
                let idx = row * row_size;
                my_shard.grid[idx] = updated_shard_req.right[row - 1];
            }
        }
        // Check if the other shard is directly to the right
        else if other.y == my.y && my.x + my.width == other.x && other.height == my.height {
            // Update right shadow lane (col width+1, rows 1..=height)
            for row in 1..=height {
                let idx = row * row_size + (width + 1);
                my_shard.grid[idx] = updated_shard_req.left[row - 1];
            }
        }
    }
} 