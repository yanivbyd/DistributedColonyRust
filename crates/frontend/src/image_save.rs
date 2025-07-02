use shared::be_api::Color;
use std::path::Path;
use std::process::Command;
use std::fs;
use std::process::Stdio;

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