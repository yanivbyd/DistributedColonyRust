// Global topography module for the coordinator
// This module will handle global topography-related functionality

use shared::be_api::Shard;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use shared::colony_model::ShardTopographyInfo;

pub struct GlobalTopographyInfo {
    pub total_width: usize,
    pub total_height: usize,
    pub shard_width: usize,
    pub shard_height: usize,
    pub default_value: u8,
    pub points_per_subgrid: u8,
    pub points_min_max_value: (u8, u8),
}

pub struct GlobalTopography {
    info: GlobalTopographyInfo,
}

impl GlobalTopography {
    pub fn new(info: GlobalTopographyInfo) -> Self {
        Self { info }
    }

    fn send_topography_to_local_shard(&self, _shard: Shard, _info: ShardTopographyInfo) {
        // Assume this is implemented, do not implement it here
    }

    pub fn generate_topography(&self) {
        let horizontal_count = self.info.total_width / self.info.shard_width;
        let vertical_count = self.info.total_height / self.info.shard_height;
        let shard_w = self.info.shard_width;
        let shard_h = self.info.shard_height;
        let default_value = self.info.default_value;
        let points_per_subgrid = self.info.points_per_subgrid;
        let (min_value, max_value) = self.info.points_min_max_value;

        // Border caches to ensure adjacent shards share borders
        let mut top_borders: Vec<Vec<u8>> = vec![vec![0; shard_w]; horizontal_count];
        let mut left_borders: Vec<Vec<u8>> = vec![vec![0; shard_h]; vertical_count];

        // Generate all horizontal borders (top for each row)
        for x in 0..horizontal_count {
            top_borders[x] = Self::generate_border(shard_w, default_value);
        }
        // Generate all vertical borders (left for each column)
        for y in 0..vertical_count {
            left_borders[y] = Self::generate_border(shard_h, default_value);
        }

        for y in 0..vertical_count {
            for x in 0..horizontal_count {
                let shard = Shard {
                    x: (x * shard_w) as i32,
                    y: (y * shard_h) as i32,
                    width: shard_w as i32,
                    height: shard_h as i32,
                };

                // Top border: from cache or generate
                let top_border = if y == 0 {
                    Self::generate_border(shard_w, default_value)
                } else {
                    // Bottom border of the shard above
                    Self::clone_border(&top_borders[x])
                };
                // Left border: from cache or generate
                let left_border = if x == 0 {
                    Self::generate_border(shard_h, default_value)
                } else {
                    // Right border of the shard to the left
                    Self::clone_border(&left_borders[y])
                };

                // Right border: generate and cache for next shard
                let right_border = if x + 1 < horizontal_count {
                    let border = Self::generate_border(shard_h, default_value);
                    left_borders[y] = border.clone();
                    border
                } else {
                    Self::generate_border(shard_h, default_value)
                };
                // Bottom border: generate and cache for next row
                let bottom_border = if y + 1 < vertical_count {
                    let border = Self::generate_border(shard_w, default_value);
                    top_borders[x] = border.clone();
                    border
                } else {
                    Self::generate_border(shard_w, default_value)
                };

                // Generate random points inside the shard (excluding borders)
                let points = Self::generate_points(
                    x,
                    y,
                    shard_w,
                    shard_h,
                    points_per_subgrid,
                    min_value,
                    max_value,
                );

                let topography = ShardTopographyInfo {
                    default_value,
                    top_border,
                    bottom_border,
                    left_border,
                    right_border,
                    points,
                };

                self.send_topography_to_local_shard(shard, topography);
            }
        }
    }

    // Helper: Generate a border with linear interpolation and ±1 step
    fn generate_border(len: usize, default_value: u8) -> Vec<u8> {
        if len == 0 { return vec![]; }
        let mut rng = rand::thread_rng();
        let start = default_value;
        let end = default_value;
        let mut border = vec![0u8; len];
        border[0] = start;
        border[len - 1] = end;
        for i in 1..len - 1 {
            let prev = border[i - 1] as i16;
            let step = rng.gen_range(-1..=1);
            let mut val = prev + step;
            if val < 0 { val = 0; }
            if val > 255 { val = 255; }
            border[i] = val as u8;
        }
        border
    }

    // Helper: Clone a border
    fn clone_border(border: &Vec<u8>) -> Vec<u8> {
        border.clone()
    }

    // Helper: Generate random points inside the shard (excluding borders)
    fn generate_points(
        shard_x: usize,
        shard_y: usize,
        shard_w: usize,
        shard_h: usize,
        points_per_subgrid: u8,
        min_value: u8,
        max_value: u8,
    ) -> Vec<(u16, u16, u8)> {
        let mut points = Vec::new();
        let mut rng = StdRng::seed_from_u64((shard_x as u64) << 32 | (shard_y as u64));
        for _ in 0..points_per_subgrid {
            let x = rng.gen_range(1..(shard_w as u16 - 1));
            let y = rng.gen_range(1..(shard_h as u16 - 1));
            let value = rng.gen_range(min_value..=max_value);
            points.push((x, y, value));
        }
        points
    }
}  