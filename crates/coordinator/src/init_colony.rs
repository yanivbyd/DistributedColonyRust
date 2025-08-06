use shared::be_api::{
    BackendRequest, BackendResponse, ColonyLifeInfo, GetColonyInfoRequest, 
    GetColonyInfoResponse, InitColonyRequest, InitColonyResponse, 
    InitColonyShardRequest, InitColonyShardResponse, Shard, BACKEND_PORT
};
use shared::{log, log_error};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bincode;

const COLONY_LIFE_INFO: ColonyLifeInfo = ColonyLifeInfo { 
    health_cost_per_size_unit: 3,
    eat_capacity_per_size_unit: 5
};
const TOTAL_WIDTH: i32 = 1250;
const TOTAL_HEIGHT: i32 = 750;

const SHARD_WIDTH: i32 = TOTAL_WIDTH / 5;
const SHARD_HEIGHT: i32 = TOTAL_HEIGHT / 3;

const SHARDS: [Shard; 15] = [
    Shard { x: 0, y: 0, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // top-left
    Shard { x: SHARD_WIDTH, y: 0, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // top-middle-left
    Shard { x: 2 * SHARD_WIDTH, y: 0, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // top-middle
    Shard { x: 3 * SHARD_WIDTH, y: 0, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // top-middle-right
    Shard { x: 4 * SHARD_WIDTH, y: 0, width: TOTAL_WIDTH - 4 * SHARD_WIDTH, height: SHARD_HEIGHT }, // top-right
    Shard { x: 0, y: SHARD_HEIGHT, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // mid-left
    Shard { x: SHARD_WIDTH, y: SHARD_HEIGHT, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // mid-middle-left
    Shard { x: 2 * SHARD_WIDTH, y: SHARD_HEIGHT, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // mid-middle
    Shard { x: 3 * SHARD_WIDTH, y: SHARD_HEIGHT, width: SHARD_WIDTH, height: SHARD_HEIGHT }, // mid-middle-right
    Shard { x: 4 * SHARD_WIDTH, y: SHARD_HEIGHT, width: TOTAL_WIDTH - 4 * SHARD_WIDTH, height: SHARD_HEIGHT }, // mid-right
    Shard { x: 0, y: 2 * SHARD_HEIGHT, width: SHARD_WIDTH, height: TOTAL_HEIGHT - 2 * SHARD_HEIGHT }, // bottom-left
    Shard { x: SHARD_WIDTH, y: 2 * SHARD_HEIGHT, width: SHARD_WIDTH, height: TOTAL_HEIGHT - 2 * SHARD_HEIGHT }, // bottom-middle-left
    Shard { x: 2 * SHARD_WIDTH, y: 2 * SHARD_HEIGHT, width: SHARD_WIDTH, height: TOTAL_HEIGHT - 2 * SHARD_HEIGHT }, // bottom-middle
    Shard { x: 3 * SHARD_WIDTH, y: 2 * SHARD_HEIGHT, width: SHARD_WIDTH, height: TOTAL_HEIGHT - 2 * SHARD_HEIGHT }, // bottom-middle-right
    Shard { x: 4 * SHARD_WIDTH, y: 2 * SHARD_HEIGHT, width: TOTAL_WIDTH - 4 * SHARD_WIDTH, height: TOTAL_HEIGHT - 2 * SHARD_HEIGHT }, // bottom-right
];

async fn send_message<T: serde::Serialize>(stream: &mut TcpStream, msg: &T) {
    let encoded = bincode::serialize(msg).expect("Failed to serialize message");
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).await.expect("Failed to write length");
    stream.write_all(&encoded).await.expect("Failed to write message");
}

// Helper to receive a length-prefixed message
async fn receive_message<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Option<T> {
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).await.is_err() {
        log_error!("Failed to read message length");
        return None;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    if stream.read_exact(&mut buf).await.is_err() {
        log_error!("Failed to read message body");
        return None;
    }
    bincode::deserialize(&buf).ok()
} 

async fn get_colony_info(stream: &mut TcpStream) -> Option<GetColonyInfoResponse> {
    let req = BackendRequest::GetColonyInfo(GetColonyInfoRequest);
    send_message(stream, &req).await;
    if let Some(response) = receive_message::<BackendResponse>(stream).await {
        match response {
            BackendResponse::GetColonyInfo(info) => Some(info),
            _ => None,
        }
    } else {
        None
    }
}

async fn connect_to_backend() -> TcpStream {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let stream = TcpStream::connect(&addr).await.expect("Failed to connect to backend");
    stream
}

async fn send_init_colony(stream: &mut TcpStream) {
    let init = BackendRequest::InitColony(InitColonyRequest { width: TOTAL_WIDTH, height: TOTAL_HEIGHT, colony_life_info: COLONY_LIFE_INFO });
    send_message(stream, &init).await;

    if let Some(response) = receive_message::<BackendResponse>(stream).await {
        match response {
            BackendResponse::InitColony(InitColonyResponse::Ok) => log!("[COORD] Colony initialized"),
            BackendResponse::InitColony(InitColonyResponse::ColonyAlreadyInitialized) => log!("[COORD] Colony already initialized"),
            _ => log_error!("[COORD] Unexpected response"),
        }
    }
}

async fn send_init_colony_shard(stream: &mut TcpStream, shard: Shard) {
    let req = BackendRequest::InitColonyShard(InitColonyShardRequest { shard: shard, colony_life_info: COLONY_LIFE_INFO });
    send_message(stream, &req).await;
    if let Some(response) = receive_message::<BackendResponse>(stream).await {
        match response {
            BackendResponse::InitColonyShard(InitColonyShardResponse::Ok) => {
                log!("[COORD] Shard initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized) => {
                log!("[COORD] Shard already initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized) => {
                log_error!("[COORD] Colony not initialized");
            },
            _ => log_error!("[COORD] Unexpected response to InitColonyShard"),
        }
    }
}

pub async fn initialize_colony() {
    let mut stream = connect_to_backend().await;

    // Call GetColonyInfo first
    let colony_info = get_colony_info(&mut stream).await;
    log!("[COORD] Colony info: {:?}", colony_info);
    let mut initialized_shards: Vec<Shard> = vec![];
    match colony_info {
        Some(GetColonyInfoResponse::Ok { width, height, shards }) => {
            initialized_shards = shards;
            log!("[COORD] Colony already initialized: {}x{}, {} shards", width, height, initialized_shards.len());
        },
        Some(GetColonyInfoResponse::ColonyNotInitialized) | None => {
            send_init_colony(&mut stream).await;
        }
    }

    // Only init shards that are not already initialized
    for shard in SHARDS.iter() {
        if !initialized_shards.contains(shard) {
            send_init_colony_shard(&mut stream, *shard).await;
        } else {
            log!("[COORD] Shard already initialized: ({},{},{},{})", shard.x, shard.y, shard.width, shard.height);
        }
    }

    // Init Topography, initialize and call global topography, use constants at the top of the file when needed
    use crate::global_topography::{GlobalTopography, GlobalTopographyInfo};
    let topography_info = GlobalTopographyInfo {
        total_width: TOTAL_WIDTH as usize,
        total_height: TOTAL_HEIGHT as usize,
        shard_width: SHARD_WIDTH as usize,
        shard_height: SHARD_HEIGHT as usize,
        default_value: 10, 
        points_per_subgrid: 5, 
        points_min_max_value: (100, 250), 
    };
    GlobalTopography::new(topography_info).generate_topography().await;
} 