use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, InitColonyRequest, CLIENT_TIMEOUT, GetShardImageRequest, GetShardImageResponse, Shard, InitColonyShardRequest, InitColonyShardResponse, InitColonyResponse};
use bincode;
use std::net::TcpStream;
use std::io::{Read, Write};
use std::time::Duration;
use std::thread;
use indicatif::{ProgressBar, ProgressStyle};
use shared::logging::{init_logging, log_startup, set_panic_hook};
use shared::log;
mod image_save;
use image_save::{save_colony_as_png, generate_video_from_frames};

const WIDTH: i32 = 500;
const HEIGHT: i32 = 500;

const SINGLE_SHARD: Shard = Shard { x: 0, y: 0, width: WIDTH, height: HEIGHT };

fn main() {
    init_logging("output/logs/fo.log");
    log_startup("FO");
    set_panic_hook();
    
    let args: Vec<String> = std::env::args().collect();
    let video_mode = args.iter().any(|a| a == "--video");
    let mut stream = connect_to_backend();

    send_init_colony(&mut stream);
    send_init_colony_shard(&mut stream, SINGLE_SHARD);

    thread::sleep(Duration::from_secs(1));

    if video_mode {
        let num_frames = 200;
        let pb = ProgressBar::new(num_frames);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} frames ({percent}%)")
            .expect("Invalid progress bar template")
            .progress_chars("#>-")
        );
        std::fs::create_dir_all("output").expect("Failed to create output directory");
        for _i in 0..num_frames {
            send_get_shard_image(&mut stream, SINGLE_SHARD);
            pb.inc(1);
            std::thread::sleep(Duration::from_millis(500));
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
        send_get_shard_image(&mut stream, SINGLE_SHARD);
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
            BackendResponse::InitColony(InitColonyResponse::Ok) => println!("[FO] Colony initialized"),
            BackendResponse::InitColony(InitColonyResponse::ColonyAlreadyInitialized) => println!("[FO] Colony already initialized"),
            _ => println!("[FO] Unexpected response"),
        }
    }
}

fn send_init_colony_shard(stream: &mut TcpStream, shard: Shard) {
    let req = BackendRequest::InitColonyShard(InitColonyShardRequest { shard: shard });
    send_message(stream, &req);
    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::InitColonyShard(InitColonyShardResponse::Ok) => {
                println!("[FO] Shard initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized) => {
                println!("[FO] Shard already initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized) => {
                println!("[FO] Colony not initialized");
            },
            _ => println!("[FO] Unexpected response to InitColonyShard"),
        }
    }
}

fn send_get_shard_image(stream: &mut TcpStream, shard: Shard) {
    log!("[FO] GetShardImage request: shard=({},{},{},{})", shard.x, shard.y, shard.width, shard.height);
    let req = BackendRequest::GetShardImage(GetShardImageRequest { shard: shard.clone() });
    send_message(stream, &req);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::GetShardImage(resp) => match resp {
                GetShardImageResponse::Image { image } => {
                    println!("[FO] Received GetShardImage response with {} pixels", image.len());
                    std::fs::create_dir_all("output").expect("Failed to create output directory");
                    save_colony_as_png(&image, shard.width as u32, shard.height as u32, "output/colony.png");
                    println!("[FO] Saved shard image as output/colony.png");
                },
                GetShardImageResponse::ShardNotAvailable => {
                    println!("[FO] Shard not available");
                }
            },
            _ => println!("[FO] Unexpected response"),
        }
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