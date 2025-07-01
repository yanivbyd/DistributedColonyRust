use shared::Color;
use std::path::Path;

pub fn save_colony_as_png(colors: &[Color], width: u32, height: u32, path: &str) {
    use image::{RgbImage, Rgb};
    let mut img = RgbImage::new(width, height);
    for (i, color) in colors.iter().enumerate() {
        let x = (i as u32) % width;
        let y = (i as u32) / width;
        img.put_pixel(x, y, Rgb([color.red, color.green, color.blue]));
    }
    img.save(Path::new(path)).expect("Failed to save PNG");
} 