use crate::colony_shard::ColonyShard;
use noise::{NoiseFn, Perlin};
use rand::Rng;

pub struct Topography;

impl Topography {
    pub fn init_topography(shard: &mut ColonyShard) {
        let width = (shard.shard.width + 2) as usize;
        let height = (shard.shard.height + 2) as usize;
        let mut rng = rand::thread_rng();
        let perlin = Perlin::new(rng.gen());
        let scale = 0.08; // Lower = larger features, higher = more detail
        let offset_x = rng.gen_range(0.0..1000.0);
        let offset_y = rng.gen_range(0.0..1000.0);

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let nx = offset_x + x as f64 * scale;
                let ny = offset_y + y as f64 * scale;
                let noise_val = perlin.get([nx, ny]);
                // Map noise_val (-1..1) to 0..1, then to 1..10 (invert for valleys/peaks)
                let mut norm = 1.0 - ((noise_val + 1.0) / 2.0);
                if norm < 0.0 { norm = 0.0; }
                if norm > 1.0 { norm = 1.0; }
                let extra = (1.0 + norm * 9.0).round() as u8;
                shard.grid[idx].extra_food_per_tick = extra;
                shard.grid[idx].food = shard.grid[idx].extra_food_per_tick;
                // shard.grid[idx].color = Color { red: 0, green: extra * 25, blue: 0 };
            }
        }
    }
}
