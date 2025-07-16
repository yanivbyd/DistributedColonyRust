use shared::be_api::Color;
use std::path::Path;
use std::process::Command;
use std::fs;
use std::process::Stdio;
use shared::be_api::Shard;

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
    let output_dir = Path::new("output");
    let video_file = Path::new(video_path).file_name().unwrap();
    let frame_file_pattern = Path::new(frame_pattern).file_name().unwrap();
    let ffmpeg_args = [
        "-y",
        "-framerate", "5",
        "-i", frame_file_pattern.to_str().unwrap(),
        "-c:v", "libx264",
        "-pix_fmt", "yuv420p",
        video_file.to_str().unwrap()
    ];
    let status = Command::new("ffmpeg")
        .current_dir(output_dir)
        .args(&ffmpeg_args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let success = match status {
        Ok(s) if s.success() => true,
        _ => false,
    };
    if success {
        // Remove all frame files using a shell command for simplicity
        let _ = Command::new("sh")
            .arg("-c")
            .arg("rm output/frame_*.png")
            .status();
    }
    success
}

/// Combines a list of shard images (as color vectors) into a single color vector for the full image.
/// Each image must correspond to its Shard definition in the same order.
/// Shards must not overlap and must fully cover the output image.
pub fn combine_shards(
    images: &[Vec<Color>],
    shards: &[Shard],
    full_width: u32,
    full_height: u32,
) -> Vec<Color> {
    let mut combined = vec![Color { red: 0, green: 0, blue: 0 }; (full_width * full_height) as usize];
    for (img, shard) in images.iter().zip(shards.iter()) {
        for y in 0..shard.height {
            for x in 0..shard.width {
                let global_x = shard.x as u32 + x as u32;
                let global_y = shard.y as u32 + y as u32;
                let global_idx = (global_y * full_width + global_x) as usize;
                let local_idx = (y as u32 * shard.width as u32 + x as u32) as usize;
                if global_x < full_width && global_y < full_height && local_idx < img.len() {
                    combined[global_idx] = img[local_idx];
                }
            }
        }
    }
    combined
} 