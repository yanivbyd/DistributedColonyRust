use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use shared::be_api::{BackendRequest, BackendResponse, GetShardImageResponse, InitColonyShardResponse, InitColonyRequest, GetShardImageRequest, InitColonyShardRequest, InitColonyResponse, GetColonyInfoRequest, GetColonyInfoResponse, UpdatedShardContentsRequest, UpdatedShardContentsResponse, GetShardLayerRequest, GetShardLayerResponse, InitShardTopographyRequest, InitShardTopographyResponse, GetShardCurrentTickRequest, GetShardCurrentTickResponse, ApplyEventRequest, ApplyEventResponse};
use bincode;
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error};
use rand::{SeedableRng, rngs::SmallRng};

mod colony;
mod be_ticker;
mod colony_shard;
mod shard_utils;
mod shard_storage;
mod be_colony_events;
mod shard_topography;
mod backend_config;

use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use crate::shard_topography::ShardTopography;


// Debug logging macro that does nothing by default
macro_rules! log_debug {
    ($($arg:tt)*) => {};
}

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

fn call_label(response: &BackendResponse) -> &'static str {
    match response {
        BackendResponse::Ping => "Ping",
        BackendResponse::InitColony(_) => "InitColony",
        BackendResponse::GetShardImage(_) => "GetShardImage",
        BackendResponse::GetShardLayer(_) => "GetShardLayer",
        BackendResponse::InitColonyShard(_) => "InitColonyShard",
        BackendResponse::GetColonyInfo(_) => "GetColonyInfo",
        BackendResponse::UpdatedShardContents(_) => "UpdatedShardContents",
        BackendResponse::InitShardTopography(_) => "InitShardTopography",
        BackendResponse::GetShardCurrentTick(_) => "GetShardCurrentTick",
        BackendResponse::ApplyEvent(_) => "ApplyEvent",
    }
}

async fn send_response(framed: &mut FramedStream, response: BackendResponse) {
    let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
    let label = call_label(&response);
    if let Err(e) = framed.send(encoded.into()).await {
        log_error!("Failed to send {} response: {}", label, e);
    } else {
        log_debug!("Sent {} response", label);
    }
}

async fn handle_client(socket: TcpStream) {
    log_debug!("handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log_debug!("handle_client: received bytes");
        let response = match bincode::deserialize::<BackendRequest>(&bytes) {
            Ok(BackendRequest::Ping) => handle_ping().await,
            Ok(BackendRequest::InitColony(req)) => handle_init_colony(req).await,
            Ok(BackendRequest::GetShardImage(req)) => handle_get_shard_image(req).await,
            Ok(BackendRequest::GetShardLayer(req)) => handle_get_shard_layer(req).await,
            Ok(BackendRequest::InitColonyShard(req)) => handle_init_colony_shard(req).await,
            Ok(BackendRequest::GetColonyInfo(req)) => handle_get_colony_info(req).await,
            Ok(BackendRequest::UpdatedShardContents(req)) => handle_updated_shard_contents(req).await,
            Ok(BackendRequest::InitShardTopography(req)) => handle_init_shard_topography(req).await,
            Ok(BackendRequest::GetShardCurrentTick(req)) => handle_get_shard_current_tick(req).await,
            Ok(BackendRequest::ApplyEvent(req)) => handle_apply_event(req).await,
            Err(e) => {
                log_error!("Failed to deserialize BackendRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
    log_debug!("handle_client: connection closed");
}

async fn handle_ping() -> BackendResponse {
    BackendResponse::Ping
}

async fn handle_init_colony(req: InitColonyRequest) -> BackendResponse {
    if Colony::is_initialized() {
        BackendResponse::InitColony(InitColonyResponse::ColonyAlreadyInitialized)
    } else {
        Colony::init(&req);
        BackendResponse::InitColony(InitColonyResponse::Ok)
    }
}

async fn handle_init_colony_shard(req: InitColonyShardRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized)
    } else if Colony::instance().has_shard(req.shard) {
        BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized)
    } else if !Colony::instance().is_valid_shard_dimensions(&req.shard) {
        BackendResponse::InitColonyShard(InitColonyShardResponse::InvalidShardDimensions)
    } else {
        let mut rng = SmallRng::from_entropy();
        Colony::instance().add_shard(ShardUtils::new_colony_shard(&req.shard, &req.colony_life_info, &mut rng));
        BackendResponse::InitColonyShard(InitColonyShardResponse::Ok)
    }
}

async fn handle_get_shard_image(req: GetShardImageRequest) -> BackendResponse {
    log_debug!("GetShardImage request: shard=({},{},{},{})", req.shard.x, req.shard.y, req.shard.width, req.shard.height);
    if ! Colony::is_initialized() {
        return BackendResponse::GetShardImage(GetShardImageResponse::ShardNotAvailable);
    }
    let colony = Colony::instance();
    if let Some(shard) = &colony.get_colony_shard(&req.shard) {
        match ShardUtils::get_shard_image(shard, &req.shard) {
            Some(image) => BackendResponse::GetShardImage(GetShardImageResponse::Image { image }),
            None => BackendResponse::GetShardImage(GetShardImageResponse::ShardNotAvailable),
        }
    } else {
        BackendResponse::GetShardImage(GetShardImageResponse::ShardNotAvailable)
    }
}

async fn handle_get_shard_layer(req: GetShardLayerRequest) -> BackendResponse {
    log_debug!("GetShardLayer request: shard=({},{},{},{}), layer={:?}", req.shard.x, req.shard.y, req.shard.width, req.shard.height, req.layer);
    if ! Colony::is_initialized() {
        return BackendResponse::GetShardImage(GetShardImageResponse::ShardNotAvailable);
    }
    let colony = Colony::instance();
    if let Some(shard) = &colony.get_colony_shard(&req.shard) {
        match ShardUtils::get_shard_layer(shard, &req.shard, &req.layer) {
            Some(data) => BackendResponse::GetShardLayer(GetShardLayerResponse::Ok { data }),
            None => BackendResponse::GetShardLayer(GetShardLayerResponse::ShardNotAvailable),
        }
    } else {
        BackendResponse::GetShardLayer(GetShardLayerResponse::ShardNotAvailable)
    }
}

async fn handle_get_colony_info(_req: GetColonyInfoRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::ColonyNotInitialized)
    } else {
        let colony = Colony::instance();
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::Ok {
            width: colony._width,
            height: colony._height,
            shards: colony.shards.iter().map(|cs| cs.shard).collect(),
        })
    }
}

async fn handle_updated_shard_contents(req: UpdatedShardContentsRequest) -> BackendResponse {
    log_debug!("UpdatedShardContents request: shard=({},{},{},{})", req.updated_shard.x, req.updated_shard.y, req.updated_shard.width, req.updated_shard.height);
    
    if !Colony::is_initialized() {
        return BackendResponse::UpdatedShardContents(UpdatedShardContentsResponse {});
    }
    
    let mut colony = Colony::instance();    
    for shard in colony.shards.iter_mut() {
        if ShardUtils::is_adjacent_shard(&req.updated_shard, &shard.shard) {
            ShardUtils::updated_shard_contents(shard, &req);
        }
    }
    
    BackendResponse::UpdatedShardContents(UpdatedShardContentsResponse {})
}

async fn handle_init_shard_topography(req: InitShardTopographyRequest) -> BackendResponse {
    log_debug!("InitShardTopography request: shard=({},{},{},{})", req.shard.x, req.shard.y, req.shard.width, req.shard.height);
    
    if !Colony::is_initialized() {
        return BackendResponse::InitShardTopography(InitShardTopographyResponse::ShardNotInitialized);
    }
    
    let mut colony = Colony::instance();
    if let Some(shard) = colony.get_colony_shard_mut(&req.shard) {
        match ShardTopography::init_shard_topography_from_data(shard, &req.topography_data) {
            Ok(()) => BackendResponse::InitShardTopography(InitShardTopographyResponse::Ok),
            Err(_) => BackendResponse::InitShardTopography(InitShardTopographyResponse::InvalidTopographyData),
        }
    } else {
        BackendResponse::InitShardTopography(InitShardTopographyResponse::ShardNotInitialized)
    }
}

async fn handle_get_shard_current_tick(req: GetShardCurrentTickRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::GetShardCurrentTick(GetShardCurrentTickResponse::ColonyNotInitialized)
    } else {
        let colony = Colony::instance();
        if let Some(shard) = colony.get_colony_shard(&req.shard) {
            BackendResponse::GetShardCurrentTick(GetShardCurrentTickResponse::Ok {
                current_tick: shard.get_current_tick(),
            })
        } else {
            BackendResponse::GetShardCurrentTick(GetShardCurrentTickResponse::ShardNotAvailable)
        }
    }
}

async fn handle_apply_event(req: ApplyEventRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::ApplyEvent(ApplyEventResponse::ColonyNotInitialized)
    } else {
        let mut colony = Colony::instance();
        let mut rng = shared::utils::new_random_generator();
        crate::be_colony_events::apply_event(&mut rng, &mut colony, &req.event);
        BackendResponse::ApplyEvent(ApplyEventResponse::Ok)
    }
}

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <hostname> <port>", args[0]);
        eprintln!("Example: {} 127.0.0.1 8082", args[0]);
        std::process::exit(1);
    }
    
    let hostname = args[1].clone();
    let port: u16 = args[2].parse().expect("Port must be a valid number");
    
    // Initialize global variables
    backend_config::set_backend_hostname(hostname.clone());
    backend_config::set_backend_port(port);
    
    // Validate that this backend's hostname and port are in the cluster topology
    let topology = shared::cluster_topology::ClusterTopology::get_instance();
    let backend_hosts = topology.get_all_backend_hosts();
    let this_host = shared::cluster_topology::HostInfo::new(hostname.clone(), port);
    
    if !backend_hosts.contains(&this_host) {
        panic!("Backend hostname '{}' and port '{}' not found in cluster topology. Available backend hosts: {:?}", 
               hostname, port, backend_hosts.iter().map(|h| h.to_address()).collect::<Vec<_>>());
    }
    
    init_logging("output/logs/be.log");
    log_startup("BE");
    set_panic_hook();
    shared::metrics::start_metrics_endpoint();
    be_ticker::start_be_ticker();
    let addr = format!("{}:{}", hostname, port);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    log_debug!("Listening on {}", addr);

    loop {
        log_debug!("Waiting for connection...");
        match listener.accept().await {
            Ok((socket, _)) => {
                log_debug!("Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("Connection failed: {}", e),
        }
    }
} 