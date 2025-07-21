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
        let scale = 0.05;
        let offset_x = rng.gen_range(0.0..1000.0);
        let offset_y = rng.gen_range(0.0..1000.0);

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let nx = offset_x + x as f64 * scale;
                let ny = offset_y + y as f64 * scale;

                let raw_noise = fbm(&perlin, nx, ny, 5, 0.5, 2.0);
                let mut norm = ((raw_noise + 1.0) / 2.0).powf(1.5);
                norm = norm.clamp(0.0, 1.0);

                let extra = (1.0 + norm * 40.0).round() as u8;
                shard.grid[idx].extra_food_per_tick = extra;
                shard.grid[idx].food = extra;
                // shard.grid[idx].color = Color { red: 0, green: extra * 25, blue: 0 };
            }
        }
    }
}

fn fbm(perlin: &Perlin, x: f64, y: f64, octaves: usize, persistence: f64, lacunarity: f64) -> f64 {
    let mut total = 0.0;
    let mut frequency = 1.0;
    let mut amplitude = 1.0;
    let mut max_value = 0.0;

    for _ in 0..octaves {
        total += perlin.get([x * frequency, y * frequency]) * amplitude;
        max_value += amplitude;
        amplitude *= persistence;
        frequency *= lacunarity;
    }

    total / max_value
}
