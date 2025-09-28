use shared::be_api::{
    BackendRequest, BackendResponse, ColonyLifeRules, GetColonyInfoRequest, 
    GetColonyInfoResponse, InitColonyRequest, InitColonyResponse, 
    InitColonyShardRequest, InitColonyShardResponse, Shard
};
use shared::cluster_topology::ClusterTopology;
use shared::{log, log_error};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bincode;
use crate::coordinator_storage::{CoordinatorStorage, CoordinatorStoredInfo, ColonyStatus};
use crate::coordinator_context::CoordinatorContext;

pub const COLONY_LIFE_INITIAL_RULES: ColonyLifeRules = ColonyLifeRules { 
    health_cost_per_size_unit: 2,
    eat_capacity_per_size_unit: 5,
    health_cost_if_can_kill: 10,
    health_cost_if_can_move: 5,
    mutation_chance: 100,
};


fn generate_shards() -> Vec<Shard> {
    ClusterTopology::get_instance().get_all_shards()
}

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

async fn connect_to_backend(hostname: &str, port: u16) -> TcpStream {
    let addr = format!("{}:{}", hostname, port);
    let stream = TcpStream::connect(&addr).await.expect(&format!("Failed to connect to backend at {}", addr));
    stream
}

async fn send_init_colony(stream: &mut TcpStream) {
    let init = BackendRequest::InitColony(InitColonyRequest { 
        width: ClusterTopology::get_width_in_shards() * ClusterTopology::get_shard_width(), 
        height: ClusterTopology::get_height_in_shards() * ClusterTopology::get_shard_height(), 
        colony_life_rules: COLONY_LIFE_INITIAL_RULES 
    });
    send_message(stream, &init).await;

    if let Some(response) = receive_message::<BackendResponse>(stream).await {
        match response {
            BackendResponse::InitColony(InitColonyResponse::Ok) => log!("Colony initialized"),
            BackendResponse::InitColony(InitColonyResponse::ColonyAlreadyInitialized) => log!("Colony already initialized"),
            _ => log_error!("Unexpected response"),
        }
    }
}

async fn send_init_colony_shard(stream: &mut TcpStream, shard: Shard) {
    let req = BackendRequest::InitColonyShard(InitColonyShardRequest { shard: shard, colony_life_rules: COLONY_LIFE_INITIAL_RULES });
    send_message(stream, &req).await;
    if let Some(response) = receive_message::<BackendResponse>(stream).await {
        match response {
            BackendResponse::InitColonyShard(InitColonyShardResponse::Ok) => {
                log!("Shard initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized) => {
                log!("Shard already initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized) => {
                log_error!("Colony not initialized");
            },
            _ => log_error!("Unexpected response to InitColonyShard"),
        }
    }
}

pub async fn initialize_colony() {
    // Step 1: Retrieve coordination info and initialize context
    let stored_info = CoordinatorStorage::retrieve(crate::coordinator_storage::COORDINATOR_STATE_FILE)
        .unwrap_or_else(|| {
            log!("No existing coordination info found, starting fresh");
            CoordinatorStoredInfo::new()
        });
    
    CoordinatorContext::initialize_with_stored_info(stored_info);
    let context = CoordinatorContext::get_instance();
    
    log!("Starting colony initialization with status: {:?}", context.get_coord_stored_info().status);
    
    let topology = ClusterTopology::get_instance();
    let backend_hosts = topology.get_all_backend_hosts();
    
    // Step 1: Initialize colony if not already done - should ALWAYS be done
    log!("Step 1: Initializing colony");
    
    // Try to get colony info from the first backend
    let mut stream = connect_to_backend(&backend_hosts[0].hostname, backend_hosts[0].port).await;
    let colony_info = get_colony_info(&mut stream).await;
    log!("Colony info: {:?}", colony_info);
    
    match colony_info {
        Some(GetColonyInfoResponse::Ok { width, height, shards: _, colony_life_rules, .. }) => {
            let mut coord_info = context.get_coord_stored_info();
            coord_info.colony_width = Some(width);
            coord_info.colony_height = Some(height);
            coord_info.colony_life_rules = colony_life_rules;
        },
        Some(GetColonyInfoResponse::ColonyNotInitialized) | None => {
            // Initialize colony on all backends
            for backend_host in backend_hosts.iter() {
                let mut stream = connect_to_backend(&backend_host.hostname, backend_host.port).await;
                send_init_colony(&mut stream).await;
            }
            let mut coord_info = context.get_coord_stored_info();
            coord_info.colony_width = Some(ClusterTopology::get_width_in_shards() * ClusterTopology::get_shard_width());
            coord_info.colony_height = Some(ClusterTopology::get_height_in_shards() * ClusterTopology::get_shard_height());
            coord_info.colony_life_rules = Some(COLONY_LIFE_INITIAL_RULES);
        }
    }
    
    // Step 2: Initialize shards - should ALWAYS be done
    log!("Step 2: Initializing shards");
    
    let all_shards = generate_shards();
    
    // Initialize shards on their respective backends
    for shard in all_shards.iter() {
        if let Some(host_info) = topology.get_host_for_shard(shard) {
            let mut stream = connect_to_backend(&host_info.hostname, host_info.port).await;
            send_init_colony_shard(&mut stream, *shard).await;
        } else {
            log_error!("No backend found for shard {:?}", shard);
        }
    }    

    // Step 3: Initialize topography
    if matches!(context.get_coord_stored_info().status, ColonyStatus::NotInitialized) {
        log!("Step 3: Initializing topography");
        
        use crate::global_topography::{GlobalTopography, GlobalTopographyInfo};
        let topography_info = GlobalTopographyInfo {
            total_width: (ClusterTopology::get_width_in_shards() * ClusterTopology::get_shard_width()) as usize,
            total_height: (ClusterTopology::get_height_in_shards() * ClusterTopology::get_shard_height()) as usize,
            shard_width: ClusterTopology::get_shard_width() as usize,
            shard_height: ClusterTopology::get_shard_height() as usize,

            base_elevation: 5,
            river_elevation_range: 45, 
            river_influence_distance: 175.0,
            river_count_range: (10, 20),
            river_segments_range: (30, 4045),
            river_step_length_range: (20.0, 30.0),
            river_direction_change: 0.6,
            smoothing_iterations: 4,
        };
        GlobalTopography::new(topography_info).generate_topography().await;
        
        let mut coord_stored_info = context.get_coord_stored_info();
        coord_stored_info.status = ColonyStatus::TopographyInitialized;
        
        if let Err(e) = CoordinatorStorage::store(&coord_stored_info, crate::coordinator_storage::COORDINATOR_STATE_FILE) {
            log_error!("Failed to save coordination info: {}", e);
        }
    }
    
    log!("Colony initialization completed with status: {:?}", context.get_coord_stored_info().status);
} 