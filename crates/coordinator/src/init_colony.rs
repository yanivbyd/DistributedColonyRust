use shared::be_api::{
    BackendRequest, BackendResponse, ColonyLifeRules, GetColonyInfoRequest, 
    GetColonyInfoResponse, InitColonyRequest, InitColonyResponse, 
    InitColonyShardRequest, InitColonyShardResponse, Shard, StartTickingRequest, StartTickingResponse
};
use shared::cluster_topology::HostInfo;
use std::collections::HashSet;
use shared::cluster_topology::ClusterTopology;
use shared::{log, log_error};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bincode;
use backoff::{ExponentialBackoff, Error as BackoffError};
use std::time::Duration;
use std::sync::Arc;
use crate::coordinator_storage::{CoordinatorStoredInfo, ColonyStatus};
use crate::coordinator_context::CoordinatorContext;
use crate::event_logging;

pub const COLONY_LIFE_INITIAL_RULES: ColonyLifeRules = ColonyLifeRules { 
    health_cost_per_size_unit: 2,
    eat_capacity_per_size_unit: 5,
    health_cost_if_can_kill: 10,
    health_cost_if_can_move: 5,
    mutation_chance: 100,
    random_death_chance: 100,
};


fn generate_shards(topology: &ClusterTopology) -> Vec<Shard> {
    topology.get_all_shards()
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

async fn connect_to_backend(hostname: &str, port: u16) -> Result<TcpStream, std::io::Error> {
    let addr = format!("{}:{}", hostname, port);
    
    // Configure exponential backoff: start with 100ms, max 2s, max 5 retries
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(2),
        max_elapsed_time: Some(Duration::from_secs(10)),
        multiplier: 2.0,
        ..Default::default()
    };
    
    let operation = || async {
        match TcpStream::connect(&addr).await {
            Ok(stream) => Ok(stream),
            Err(e) => {
                log_error!("Failed to connect to backend at {}: {}", addr, e);
                Err(BackoffError::transient(e))
            }
        }
    };
    
    backoff::future::retry(backoff, operation).await
}

async fn send_init_colony(stream: &mut TcpStream, topology: Arc<ClusterTopology>) {
    let init = BackendRequest::InitColony(InitColonyRequest { 
        width: topology.width_in_shards() * topology.shard_width(), 
        height: topology.height_in_shards() * topology.shard_height(), 
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

async fn send_init_colony_shard(stream: &mut TcpStream, shard: Shard, topology: Arc<ClusterTopology>) {
    // Clone the topology to send to backend
    // Note: ClusterTopology is now Clone and serializable, so we can clone it directly
    let topology_clone = (*topology).clone();
    
    let req = BackendRequest::InitColonyShard(InitColonyShardRequest { 
        shard: shard, 
        colony_life_rules: COLONY_LIFE_INITIAL_RULES,
        topology: Some(topology_clone),
    });
    send_message(stream, &req).await;
    if let Some(response) = receive_message::<BackendResponse>(stream).await {
        match response {
            BackendResponse::InitColonyShard(InitColonyShardResponse::Ok) => {
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized) => {
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized) => {
                log_error!("Colony not initialized");
            },
            BackendResponse::InitColonyShard(InitColonyShardResponse::Error) => {
                log_error!("Error initializing shard (missing or invalid topology)");
            },
            _ => log_error!("Unexpected response to InitColonyShard"),
        }
    }
}

pub async fn initialize_colony() {
    // Step 1: Get or initialize context
    // Note: Context may already be initialized, so we just get the instance
    // and reset the stored info if needed
    let context = CoordinatorContext::get_instance();
    
    // Reset stored info to fresh state, but preserve instance ID and idempotency key
    // (these are set before initialize_colony is called)
    {
        let mut stored_info = context.get_coord_stored_info();
        let preserved_instance_id = stored_info.colony_instance_id.clone();
        let preserved_idempotency_key = stored_info.colony_start_idempotency_key.clone();
        let preserved_deployment_mode = stored_info.deployment_mode.clone();
        *stored_info = CoordinatorStoredInfo::new();
        stored_info.colony_instance_id = preserved_instance_id;
        stored_info.colony_start_idempotency_key = preserved_idempotency_key;
        stored_info.deployment_mode = preserved_deployment_mode;
    }
    
    log!("Starting colony initialization with status: {:?}", context.get_coord_stored_info().status);
    
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized. Cannot initialize colony.");
            return;
        }
    };
    let backend_hosts = topology.get_all_backend_hosts();
    
    // Step 1: Initialize colony if not already done - should ALWAYS be done
    log!("Step 1: Initializing colony");
    
    // Try to get colony info from the first backend
    let mut stream = match connect_to_backend(&backend_hosts[0].hostname, backend_hosts[0].port).await {
        Ok(stream) => stream,
        Err(e) => {
            log_error!("Failed to connect to backend {}:{} after retries: {}", 
                      backend_hosts[0].hostname, backend_hosts[0].port, e);
            return;
        }
    };
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
                let mut stream = match connect_to_backend(&backend_host.hostname, backend_host.port).await {
                    Ok(stream) => stream,
                    Err(e) => {
                        log_error!("Failed to connect to backend {}:{} after retries: {}", 
                                  backend_host.hostname, backend_host.port, e);
                        continue;
                    }
                };
                send_init_colony(&mut stream, topology.clone()).await;
            }
            let mut coord_info = context.get_coord_stored_info();
            coord_info.colony_width = Some(topology.width_in_shards() * topology.shard_width());
            coord_info.colony_height = Some(topology.height_in_shards() * topology.shard_height());
            coord_info.colony_life_rules = Some(COLONY_LIFE_INITIAL_RULES);
        }
    }
    
    // Step 2: Initialize shards - should ALWAYS be done
    log!("Step 2: Initializing shards");
    
    let all_shards = generate_shards(&topology);
    
    // Initialize shards on their respective backends
    for shard in all_shards.iter() {
        if let Some(host_info) = topology.get_host_for_shard(shard) {
            let mut stream = match connect_to_backend(&host_info.hostname, host_info.port).await {
                Ok(stream) => stream,
                Err(e) => {
                    log_error!("Failed to connect to backend {}:{} for shard {:?} after retries: {}", 
                              host_info.hostname, host_info.port, shard, e);
                    continue;
                }
            };
            send_init_colony_shard(&mut stream, *shard, topology.clone()).await;
        } else {
            log_error!("No backend found for shard {:?}", shard);
        }
    }    

    // Step 3: Initialize topography
    if matches!(context.get_coord_stored_info().status, ColonyStatus::NotInitialized) {
        log!("Step 3: Initializing topography");
        
        use crate::global_topography::{GlobalTopography, GlobalTopographyInfo};
        let topography_info = GlobalTopographyInfo {
            total_width: (topology.width_in_shards() * topology.shard_width()) as usize,
            total_height: (topology.height_in_shards() * topology.shard_height()) as usize,
            shard_width: topology.shard_width() as usize,
            shard_height: topology.shard_height() as usize,

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
    }
    
    log!("Colony initialization completed with status: {:?}", context.get_coord_stored_info().status);
    
    // Log colony creation event
    let rules = context.get_colony_life_rules();
    if let Err(e) = event_logging::write_colony_created_event_json(rules) {
        log_error!("Failed to write colony creation event JSON: {}", e);
    }
    
    // Step 4: Start colony ticking (coordinator ticker + notify all backends)
    start_colony_ticking().await;
}

pub async fn start_colony_ticking() {
    log!("Starting colony ticking: initiating coordinator ticker and notifying all backends");
    
    // Step 1: Start coordinator ticker
    crate::coordinator_ticker::start_coordinator_ticker();
    
    // Step 2: Get topology and all backends
    let topology_arc = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized. Cannot start colony ticking.");
            return;
        }
    };
    
    // Step 3: Send StartTicking to all unique backends
    let backend_hosts = topology_arc.get_all_backend_hosts();
    let mut unique_backends: HashSet<HostInfo> = HashSet::new();
    for host in backend_hosts.iter() {
        unique_backends.insert(host.clone());
    }
    
    let backend_count = unique_backends.len();
    for backend_host in unique_backends {
        match send_start_ticking_to_backend(&backend_host).await {
            Ok(StartTickingResponse::Ok) => {
                log!("Backend {}:{} started ticking", backend_host.hostname, backend_host.port);
            }
            Ok(StartTickingResponse::ColonyNotInitialized) => {
                log_error!("Backend {}:{} cannot start ticking: colony not initialized", 
                          backend_host.hostname, backend_host.port);
            }
            Ok(StartTickingResponse::TopologyNotInitialized) => {
                log_error!("Backend {}:{} cannot start ticking: topology not initialized", 
                          backend_host.hostname, backend_host.port);
            }
            Ok(StartTickingResponse::Error(msg)) => {
                log_error!("Backend {}:{} failed to start ticking: {}", 
                          backend_host.hostname, backend_host.port, msg);
            }
            Err(e) => {
                log_error!("Failed to send StartTicking to {}:{}: {}", 
                          backend_host.hostname, backend_host.port, e);
            }
        }
    }
    
    log!("Colony ticking started: coordinator ticker active, {} backends notified", backend_count);
}

async fn send_start_ticking_to_backend(backend_host: &HostInfo) -> Result<StartTickingResponse, String> {
    let mut stream = connect_to_backend(&backend_host.hostname, backend_host.port).await
        .map_err(|e| format!("Connection failed: {}", e))?;
    
    let request = BackendRequest::StartTicking(StartTickingRequest {});
    send_message(&mut stream, &request).await;
    
    if let Some(response) = receive_message::<BackendResponse>(&mut stream).await {
        match response {
            BackendResponse::StartTicking(resp) => Ok(resp),
            _ => Err("Unexpected response type".to_string()),
        }
    } else {
        Err("Failed to receive response".to_string())
    }
} 