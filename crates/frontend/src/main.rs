use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, InitColonyRequest, CLIENT_TIMEOUT, GetShardImageRequest, GetShardImageResponse, Shard, InitColonyShardRequest, InitColonyShardResponse, InitColonyResponse, GetColonyInfoRequest, GetColonyInfoResponse};
use bincode;
use std::net::TcpStream;
use std::io::{Read, Write};
use shared::logging::{init_logging, log_startup, set_panic_hook};
use shared::{log, log_error};
mod image_save;
use image_save::{save_colony_as_png, combine_shards};
use indicatif::{ProgressBar, ProgressStyle};

const WIDTH: i32 = 750;
const HEIGHT: i32 = 750;

const THIRD_WIDTH: i32 = WIDTH / 3;
const THIRD_HEIGHT: i32 = HEIGHT / 3;

const SHARDS: [Shard; 9] = [
    Shard { x: 0, y: 0, width: THIRD_WIDTH, height: THIRD_HEIGHT }, // top-left
    Shard { x: THIRD_WIDTH, y: 0, width: THIRD_WIDTH, height: THIRD_HEIGHT }, // top-middle
    Shard { x: 2 * THIRD_WIDTH, y: 0, width: WIDTH - 2 * THIRD_WIDTH, height: THIRD_HEIGHT }, // top-right
    Shard { x: 0, y: THIRD_HEIGHT, width: THIRD_WIDTH, height: THIRD_HEIGHT }, // mid-left
    Shard { x: THIRD_WIDTH, y: THIRD_HEIGHT, width: THIRD_WIDTH, height: THIRD_HEIGHT }, // center
    Shard { x: 2 * THIRD_WIDTH, y: THIRD_HEIGHT, width: WIDTH - 2 * THIRD_WIDTH, height: THIRD_HEIGHT }, // mid-right
    Shard { x: 0, y: 2 * THIRD_HEIGHT, width: THIRD_WIDTH, height: HEIGHT - 2 * THIRD_HEIGHT }, // bottom-left
    Shard { x: THIRD_WIDTH, y: 2 * THIRD_HEIGHT, width: THIRD_WIDTH, height: HEIGHT - 2 * THIRD_HEIGHT }, // bottom-middle
    Shard { x: 2 * THIRD_WIDTH, y: 2 * THIRD_HEIGHT, width: WIDTH - 2 * THIRD_WIDTH, height: HEIGHT - 2 * THIRD_HEIGHT }, // bottom-right
];

fn main() {
    init_logging("output/logs/fo.log");
    log_startup("FO");
    set_panic_hook();
    
    let args: Vec<String> = std::env::args().collect();
    let video_mode = args.iter().any(|a| a == "--video");
    let mut stream = connect_to_backend();

    // Call GetColonyInfo first
    let colony_info = get_colony_info(&mut stream);
    let mut initialized_shards: Vec<Shard> = vec![];
    match colony_info {
        Some(GetColonyInfoResponse::Ok { width, height, shards }) => {
            initialized_shards = shards;
            log!("[FO] Colony already initialized: {}x{}, {} shards", width, height, initialized_shards.len());
        },
        Some(GetColonyInfoResponse::ColonyNotInitialized) | None => {
            send_init_colony(&mut stream);
        }
    }

    // Only init shards that are not already initialized
    for shard in SHARDS.iter() {
        if !initialized_shards.contains(shard) {
            send_init_colony_shard(&mut stream, *shard);
        } else {
            log!("[FO] Shard already initialized: ({},{},{},{})", shard.x, shard.y, shard.width, shard.height);
        }
    }

    std::thread::sleep(std::time::Duration::from_secs(1));

    if video_mode {
        let num_frames = 20;
        let pb = ProgressBar::new(num_frames);
        pb.set_style(ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} frames ({percent}%)")
            .expect("Invalid progress bar template")
            .progress_chars("#>-")
        );
        std::fs::create_dir_all("output").expect("Failed to create output directory");
        for i in 0..num_frames {
            if let Some(combined) = get_combined_colony_image(&mut stream) {
                let frame_path = format!("output/frame_{:02}.png", i);
                save_colony_as_png(&combined, WIDTH as u32, HEIGHT as u32, &frame_path);
            } else {
                return;
            }
            pb.inc(1);
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        pb.finish_with_message("Frames generated");
        let video_created = image_save::generate_video_from_frames(
            "output/colony_video.mp4",
            "output/frame_%02d.png"
        );
        if video_created {
            println!("[FO] Video created as output/colony_video.mp4");
        } else {
            eprintln!("[FO] ffmpeg failed");
        }
    } else {
        if let Some(combined) = get_combined_colony_image(&mut stream) {
            std::fs::create_dir_all("output").expect("Failed to create output directory");
            save_colony_as_png(&combined, WIDTH as u32, HEIGHT as u32, "output/colony.png");
            println!("[FO] Saved combined colony image as output/colony.png");
        }
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
            BackendResponse::InitColony(InitColonyResponse::ColonyAlreadyInitialized) => log!("[FO] Colony already initialized"),
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
                log!("[FO] Shard initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized) => {
                log!("[FO] Shard already initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized) => {
                log_error!("[FO] Colony not initialized");
            },
            _ => println!("[FO] Unexpected response to InitColonyShard"),
        }
    }
}

fn get_shard_image_colors(stream: &mut TcpStream, shard: Shard) -> Option<Vec<shared::be_api::Color>> {
    log!("[FO] GetShardImage request: shard=({},{},{},{})", shard.x, shard.y, shard.width, shard.height);
    let req = BackendRequest::GetShardImage(GetShardImageRequest { shard: shard.clone() });
    send_message(stream, &req);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::GetShardImage(resp) => match resp {
                GetShardImageResponse::Image { image } => {
                    log!("[FO] Received GetShardImage response with {} pixels", image.len());
                    Some(image)
                },
                GetShardImageResponse::ShardNotAvailable => {
                    log!("[FO] Shard not available");
                    None
                }
            },
            _ => {
                log_error!("[FO] Unexpected response");
                None
            },
        }
    } else {
        None
    }
}

fn get_combined_colony_image(stream: &mut TcpStream) -> Option<Vec<shared::be_api::Color>> {
    let mut images = Vec::with_capacity(SHARDS.len());
    for shard in SHARDS.iter() {
        if let Some(colors) = get_shard_image_colors(stream, *shard) {
            images.push(colors);
        } else {
            println!("[FO] Failed to get image for shard: ({},{},{},{})", shard.x, shard.y, shard.width, shard.height);
            return None;
        }
    }
    Some(combine_shards(&images, &SHARDS, WIDTH as u32, HEIGHT as u32))
}

fn get_colony_info(stream: &mut TcpStream) -> Option<GetColonyInfoResponse> {
    let req = BackendRequest::GetColonyInfo(GetColonyInfoRequest);
    send_message(stream, &req);
    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::GetColonyInfo(info) => Some(info),
            _ => None,
        }
    } else {
        None
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