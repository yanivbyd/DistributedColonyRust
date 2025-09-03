use rand::rngs::SmallRng;
use rand::{thread_rng, SeedableRng, Rng};

/// Creates a new SmallRng instance seeded from the thread-local random number generator.
/// This provides a fast, non-cryptographic RNG that's suitable for most simulation purposes.
pub fn new_random_generator() -> SmallRng {
    SmallRng::from_rng(&mut thread_rng()).unwrap()
}

pub fn random_chance(rng: &mut SmallRng, out_of: u32) -> bool {
    rng.gen_range(1..=out_of) == 1
}

/// Generates a random color that is guaranteed to be visually distinct from white.
/// This ensures colors are bright and visible for creatures and other game elements.
pub fn random_color(rng: &mut SmallRng) -> crate::colony_model::Color {
    // Generate random RGB values, but ensure at least one channel is significantly low
    // to avoid colors that are too close to white (255, 255, 255)
    let mut red = rng.gen_range(0..=255);
    let mut green = rng.gen_range(0..=255);
    let mut blue = rng.gen_range(0..=255);
    
    // Ensure the color is not too close to white by making at least one channel
    // significantly lower (below 200)
    let max_channel = red.max(green).max(blue);
    if max_channel > 240 {
        // Randomly choose one channel to reduce
        match rng.gen_range(0..3) {
            0 => red = rng.gen_range(0..180),
            1 => green = rng.gen_range(0..180),
            _ => blue = rng.gen_range(0..180),
        }
    }
    
    crate::colony_model::Color { red, green, blue }
}
