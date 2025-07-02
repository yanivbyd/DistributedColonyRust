use shared::be_api::Color;
use std::path::Path;
use std::process::Command;
use std::fs;

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

pub fn generate_video_from_frames(video_path: &str, frame_pattern: &str) -> bool {
    // Remove the video file if it exists
    let _ = fs::remove_file(video_path);
    let status = Command::new("ffmpeg")
        .args(&["-y", "-framerate", "10", "-i", frame_pattern, "-c:v", "libx264", "-pix_fmt", "yuv420p", video_path])
        .status();
    match status {
        Ok(s) if s.success() => true,
        _ => false,
    }
} 