use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, InitColonyRequest, CLIENT_TIMEOUT, GetSubImageRequest};
use bincode;
use std::net::TcpStream;
use std::io::{Read, Write};
use std::time::Duration;
use std::thread;
use indicatif::{ProgressBar, ProgressStyle};
mod image_save;
use image_save::{save_colony_as_png, generate_video_from_frames};

const WIDTH: i32 = 500;
const HEIGHT: i32 = 500;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let video_mode = args.iter().any(|a| a == "--video");
    let mut stream = connect_to_backend();

    send_init_colony(&mut stream);
    thread::sleep(Duration::from_secs(1));

    if video_mode {
        let num_frames = 50;
        let pb = ProgressBar::new(num_frames);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} frames ({percent}%)")
            .expect("Invalid progress bar template")
            .progress_chars("#>-")
        );
        std::fs::create_dir_all("output").expect("Failed to create output directory");
        for i in 0..num_frames {
            send_get_sub_image_with_name(&mut stream, 0, 0, WIDTH, HEIGHT, &format!("output/frame_{:02}.png", i), true);
            pb.inc(1);
            std::thread::sleep(Duration::from_millis(200));
        }
        pb.finish_with_message("Frames generated");
        // Use helper to create video
        let video_created = generate_video_from_frames(
            "output/colony_video.mp4",
            "output/frame_%02d.png"
        );
        if video_created {
            println!("[FO] Video created as output/colony_video.mp4");
        } else {
            eprintln!("[FO] ffmpeg failed");
        }
    } else {
        send_get_sub_image(&mut stream, 0, 0, WIDTH, HEIGHT);
    }
}

fn connect_to_backend() -> TcpStream {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let stream = TcpStream::connect(&addr).expect("Failed to connect to backend");
    stream
        .set_read_timeout(Some(CLIENT_TIMEOUT))
        .expect("set_read_timeout call failed");
    stream
}

fn send_init_colony(stream: &mut TcpStream) {
    let init = BackendRequest::InitColony(InitColonyRequest { width: WIDTH, height: HEIGHT });
    send_message(stream, &init);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::InitColony => println!("[FO] Received InitColony response"),
            _ => println!("[FO] Unexpected response"),
        }
    }
}

fn send_get_sub_image(stream: &mut TcpStream, x: i32, y: i32, width: i32, height: i32) {
    let req = BackendRequest::GetSubImage(GetSubImageRequest { x, y, width, height });
    send_message(stream, &req);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::GetSubImage(resp) => {
                println!("[FO] Received GetSubImage response with {} pixels", resp.colors.len());
                std::fs::create_dir_all("output").expect("Failed to create output directory");
                save_colony_as_png(&resp.colors, width as u32, height as u32, "output/colony.png");
                println!("[FO] Saved sub-image as output/colony.png");
            }
            _ => println!("[FO] Unexpected response"),
        }
    }
}

fn send_get_sub_image_with_name(stream: &mut TcpStream, x: i32, y: i32, width: i32, height: i32, filename: &str, quiet: bool) {
    let req = BackendRequest::GetSubImage(GetSubImageRequest { x, y, width, height });
    send_message(stream, &req);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::GetSubImage(resp) => {
                save_colony_as_png(&resp.colors, width as u32, height as u32, filename);
                if !quiet {
                    println!("[FO] Received GetSubImage response with {} pixels", resp.colors.len());
                    println!("[FO] Saved sub-image as {}", filename);
                }
            }
            _ => if !quiet { println!("[FO] Unexpected response"); },
        }
    } else if !quiet {
        println!("[FO] Failed to receive GetSubImage response");
    }
}

// Helper to send a length-prefixed message
fn send_message<T: serde::Serialize>(stream: &mut TcpStream, msg: &T) {
    let encoded = bincode::serialize(msg).expect("Failed to serialize message");
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).expect("Failed to write length");
    stream.write_all(&encoded).expect("Failed to write message");
}

// Helper to receive a length-prefixed message
fn receive_message<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Option<T> {
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).is_err() {
        println!("[FO] Failed to read message length");
        return None;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    if stream.read_exact(&mut buf).is_err() {
        println!("[FO] Failed to read message body");
        return None;
    }
    bincode::deserialize(&buf).ok()
} 