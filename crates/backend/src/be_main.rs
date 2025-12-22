use shared::log;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use shared::be_api::{BackendRequest, BackendResponse, InitColonyShardResponse, InitColonyRequest, InitColonyShardRequest, InitColonyResponse, GetColonyInfoRequest, GetColonyInfoResponse, UpdatedShardContentsRequest, UpdatedShardContentsResponse, InitShardTopographyRequest, InitShardTopographyResponse, GetShardCurrentTickRequest, GetShardCurrentTickResponse, ApplyEventRequest, ApplyEventResponse, GetShardStatsRequest, GetShardStatsResponse, StartTickingRequest, StartTickingResponse};
use bincode;
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error};
use shared::cluster_topology::{DiscoveredTopology, NodeType, NodeAddress, start_periodic_discovery, ClusterTopology, HostInfo};
use shared::cluster_registry::{ClusterRegistry, create_cluster_registry, get_instance};
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq)]
enum DeploymentMode {
    Localhost,
    Aws,
}

impl DeploymentMode {
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "localhost" => Ok(DeploymentMode::Localhost),
            "aws" => Ok(DeploymentMode::Aws),
            _ => Err(format!("Invalid deployment mode: {}. Must be 'localhost' or 'aws'", s)),
        }
    }
}

mod colony;
mod be_ticker;
mod colony_shard;
mod shard_utils;
mod shard_storage;
mod be_colony_events;
mod shard_topography;
mod backend_config;
mod backend_client;
mod http_server;

use crate::be_colony_events::apply_event;
use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use crate::shard_topography::ShardTopography;
use crate::http_server::start_http_server;
use crate::backend_config::{get_backend_hostname, get_backend_port};

// Track if topology has been initialized from routing table
static TOPOLOGY_INITIALIZED: OnceLock<bool> = OnceLock::new();

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

const BUILD_VERSION: &str = match option_env!("BUILD_VERSION") {
    Some(value) => value,
    None => "unknown",
};

fn call_label(response: &BackendResponse) -> &'static str {
    match response {
        BackendResponse::Ping => "Ping",
        BackendResponse::InitColony(_) => "InitColony",
        BackendResponse::GetShardStats(_) => "GetShardStats",
        BackendResponse::InitColonyShard(_) => "InitColonyShard",
        BackendResponse::GetColonyInfo(_) => "GetColonyInfo",
        BackendResponse::UpdatedShardContents(_) => "UpdatedShardContents",
        BackendResponse::InitShardTopography(_) => "InitShardTopography",
        BackendResponse::GetShardCurrentTick(_) => "GetShardCurrentTick",
        BackendResponse::ApplyEvent(_) => "ApplyEvent",
        BackendResponse::StartTicking(_) => "StartTicking",
    }
}

async fn send_response(framed: &mut FramedStream, response: BackendResponse) {
    let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
    let label = call_label(&response);
    if let Err(e) = framed.send(encoded.into()).await {
        log_error!("Failed to send {} response: {}", label, e);
    } else {
    }
}

async fn handle_client(socket: TcpStream) {
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    loop {
        match framed.next().await {
            Some(Ok(bytes)) => {
                let response = match bincode::deserialize::<BackendRequest>(&bytes) {
                    Ok(BackendRequest::Ping) => handle_ping().await,
                    Ok(BackendRequest::InitColony(req)) => handle_init_colony(req).await,
                    Ok(BackendRequest::InitColonyShard(req)) => handle_init_colony_shard(req).await,
                    Ok(BackendRequest::GetColonyInfo(req)) => handle_get_colony_info(req).await,
                    Ok(BackendRequest::UpdatedShardContents(req)) => handle_updated_shard_contents(req).await,
                    Ok(BackendRequest::InitShardTopography(req)) => handle_init_shard_topography(req).await,
                    Ok(BackendRequest::GetShardCurrentTick(req)) => handle_get_shard_current_tick(req).await,
                    Ok(BackendRequest::GetShardStats(req)) => handle_get_shard_stats(req).await,
                    Ok(BackendRequest::ApplyEvent(req)) => handle_apply_event(req).await,
                    Ok(BackendRequest::StartTicking(req)) => handle_start_ticking(req).await,
                    Err(e) => {
                        log_error!("Failed to deserialize BackendRequest: {}", e);
                        continue;
                    }
                };
                send_response(&mut framed, response).await;
            }
            Some(Err(e)) => {
                log_error!("handle_client: error reading from connection: {}", e);
                break;
            }
            None => {
                break;
            }
        }
    }
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
    // Initialize topology from ClusterTopology object on first call
    let topology_initialized = TOPOLOGY_INITIALIZED.get().copied().unwrap_or(false);
    if !topology_initialized {
        // Extract ClusterTopology from request
        let topology = match req.topology {
            Some(t) => t,
            None => {
                log_error!("ClusterTopology missing from InitColonyShardRequest");
                return BackendResponse::InitColonyShard(InitColonyShardResponse::Error);
            }
        };
        
        // Initialize topology from ClusterTopology object
        if let Err(e) = ClusterTopology::initialize_from_topology(topology.clone()) {
            log_error!("Failed to initialize topology: {}", e);
            return BackendResponse::InitColonyShard(InitColonyShardResponse::Error);
        }
        
        // Validate that this backend's host info exists in the topology's backend hosts
        let this_backend_host = HostInfo::new(get_backend_hostname().to_string(), get_backend_port());
        let normalized_hostname = if this_backend_host.hostname == "0.0.0.0" {
            "127.0.0.1".to_string()
        } else {
            this_backend_host.hostname.clone()
        };
        let normalized_backend_host = HostInfo::new(normalized_hostname, this_backend_host.port);
        
        let backend_exists = topology.backend_hosts.iter().any(|host| {
            let normalized_host = if host.hostname == "0.0.0.0" {
                HostInfo::new("127.0.0.1".to_string(), host.port)
            } else {
                host.clone()
            };
            normalized_host == normalized_backend_host
        });
        
        if !backend_exists {
            log_error!("Backend host {}:{} not found in topology backend hosts", 
                      this_backend_host.hostname, this_backend_host.port);
            return BackendResponse::InitColonyShard(InitColonyShardResponse::Error);
        }
        
        // Mark topology as initialized
        TOPOLOGY_INITIALIZED.set(true).expect("Failed to set topology initialized flag");
        log!("Topology initialized from ClusterTopology object");
    }
    
    if !Colony::is_initialized() {
        BackendResponse::InitColonyShard(InitColonyShardResponse::ColonyNotInitialized)
    } else if Colony::instance().is_hosting_shard(req.shard) {
        BackendResponse::InitColonyShard(InitColonyShardResponse::ShardAlreadyInitialized)
    } else if !Colony::instance().is_valid_shard_dimensions(&req.shard) {
        BackendResponse::InitColonyShard(InitColonyShardResponse::InvalidShardDimensions)
    } else {
        let mut rng = shared::utils::new_random_generator();
        Colony::instance().add_hosted_shard(ShardUtils::new_colony_shard(&req.shard, &req.colony_life_rules, &mut rng));
        BackendResponse::InitColonyShard(InitColonyShardResponse::Ok)
    }
}

async fn handle_get_colony_info(_req: GetColonyInfoRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::ColonyNotInitialized)
    } else {
        let colony = Colony::instance();
        let (shards, shard_arcs) = colony.get_hosted_shards();
        
        // Get ColonyLifeRules and current_tick from the first available shard
        let (colony_life_rules, current_tick) = if let Some(first_shard_arc) = shard_arcs.first() {
            let shard = first_shard_arc.lock().unwrap();
            (Some(shard.colony_life_rules), Some(shard.current_tick))
        } else {
            (None, None)
        };
        
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::Ok {
            width: colony._width,
            height: colony._height,
            shards,
            colony_life_rules,
            current_tick,
        })
    }
}

async fn handle_get_shard_stats(req: GetShardStatsRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        return BackendResponse::GetShardStats(GetShardStatsResponse::ColonyNotInitialized);
    }
    let colony = Colony::instance();
    if let Some(shard_arc) = colony.get_hosted_colony_shard_arc(&req.shard) {
        let shard = shard_arc.lock().unwrap();
        match ShardUtils::compute_stats(&shard, &req.shard, &req.metrics) {
            Some(stats) => BackendResponse::GetShardStats(GetShardStatsResponse::Ok { stats, tick_count: shard.get_current_tick() }),
            None => BackendResponse::GetShardStats(GetShardStatsResponse::ShardNotAvailable),
        }
    } else {
        BackendResponse::GetShardStats(GetShardStatsResponse::ShardNotAvailable)
    }
}

async fn handle_updated_shard_contents(req: UpdatedShardContentsRequest) -> BackendResponse {   
    if !Colony::is_initialized() {
        return BackendResponse::UpdatedShardContents(UpdatedShardContentsResponse {});
    }
    
    let colony = Colony::instance();    
    let (_, shard_arcs) = colony.get_hosted_shards();
    for shard_arc in shard_arcs {
        let mut shard = shard_arc.lock().unwrap();
        if ShardUtils::is_adjacent_shard(&req.updated_shard, &shard.shard) {
            ShardUtils::updated_shard_contents(&mut shard, &req);
        }
    }
    
    BackendResponse::UpdatedShardContents(UpdatedShardContentsResponse {})
}

async fn handle_init_shard_topography(req: InitShardTopographyRequest) -> BackendResponse {   
    if !Colony::is_initialized() {
        return BackendResponse::InitShardTopography(InitShardTopographyResponse::ShardNotInitialized);
    }
    
    let colony = Colony::instance();
    if let Some(shard_arc) = colony.get_hosted_colony_shard_arc(&req.shard) {
        let mut shard = shard_arc.lock().unwrap();
        match ShardTopography::init_shard_topography_from_data(&mut shard, &req.topography_data) {
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
        if let Some(shard_arc) = colony.get_hosted_colony_shard_arc(&req.shard) {
            let shard = shard_arc.lock().unwrap();
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
        let colony = Colony::instance();
        let mut rng = shared::utils::new_random_generator();
        apply_event(&mut rng, &colony, &req.event);
        BackendResponse::ApplyEvent(ApplyEventResponse::Ok)
    }
}

async fn handle_start_ticking(_req: StartTickingRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        return BackendResponse::StartTicking(StartTickingResponse::ColonyNotInitialized);
    }
    
    // Check if topology is initialized using the same flag used elsewhere in the backend
    let topology_initialized = TOPOLOGY_INITIALIZED.get().copied().unwrap_or(false);
    if !topology_initialized {
        return BackendResponse::StartTicking(StartTickingResponse::TopologyNotInitialized);
    }
    
    // Start ticking (idempotent - start_be_ticker uses OnceLock to ensure only called once)
    be_ticker::start_be_ticker();
    
    BackendResponse::StartTicking(StartTickingResponse::Ok)
}

async fn create_discovered_topology(hostname: &str, rpc_port: u16) -> DiscoveredTopology {
    // In AWS mode, HTTP port comes from HTTP_PORT env var
    let http_port = std::env::var("HTTP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8085); // Default fallback
    let mut discovered_topology = DiscoveredTopology::new(
        NodeType::Backend, 
        NodeAddress::new(hostname.to_string(), hostname.to_string(), rpc_port, http_port), 
        None, 
        Vec::new()
    );
    discovered_topology.start_discovery().await;
    discovered_topology
}

fn check_port_available(port: u16) -> Result<(), String> {
    use std::net::TcpListener;
    match TcpListener::bind(format!("127.0.0.1:{}", port)) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                Err(format!("Port {} is already in use", port))
            } else {
                Err(format!("Failed to check port {}: {}", port, e))
            }
        }
    }
}

#[tokio::main]
async fn main() {
    eprintln!("BACKEND MAIN ENTERED");
    eprintln!("BUILD_VERSION={}", BUILD_VERSION);

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    eprintln!("Raw args = {:?}", args);
    
    // In AWS mode, get ports from environment variables if not provided as arguments
    let (rpc_port, http_port, hostname, deployment_mode) = if args.len() == 2 {
        // AWS mode: get from environment variables
        let deployment_mode = DeploymentMode::from_str(&args[1]).expect("Invalid deployment mode");
        if deployment_mode != DeploymentMode::Aws {
            eprintln!("Usage: {} <hostname> <rpc_port> <http_port> <deployment_mode>", args[0]);
            eprintln!("Example: {} 127.0.0.1 8084 8085 localhost", args[0]);
            eprintln!("Deployment modes: localhost, aws");
            std::process::exit(1);
        }
        let rpc_port = std::env::var("RPC_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .expect("RPC_PORT environment variable must be set in AWS mode");
        let http_port = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .expect("HTTP_PORT environment variable must be set in AWS mode");
        let hostname = std::env::var("BACKEND_HOST")
            .unwrap_or_else(|_| "0.0.0.0".to_string());
        (rpc_port, http_port, hostname, deployment_mode)
    } else if args.len() == 5 {
        // Localhost mode: get from command line arguments
        let hostname = args[1].clone();
        let rpc_port: u16 = args[2].parse().expect("RPC port must be a valid number");
        let http_port: u16 = args[3].parse().expect("HTTP port must be a valid number");
        let deployment_mode = DeploymentMode::from_str(&args[4]).expect("Invalid deployment mode");
        (rpc_port, http_port, hostname, deployment_mode)
    } else {
        eprintln!("Usage: {} <hostname> <rpc_port> <http_port> <deployment_mode>", args[0]);
        eprintln!("Example: {} 127.0.0.1 8084 8085 localhost", args[0]);
        eprintln!("Deployment modes: localhost, aws");
        eprintln!("In AWS mode, RPC_PORT and HTTP_PORT environment variables are used");
        std::process::exit(1);
    };
    
    // Validate ports are available
    if let Err(e) = check_port_available(rpc_port) {
        log_error!("RPC port validation failed: {}", e);
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    if let Err(e) = check_port_available(http_port) {
        log_error!("HTTP port validation failed: {}", e);
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    
    // Initialize global variables
    backend_config::set_backend_hostname(hostname.clone());
    backend_config::set_backend_port(rpc_port);
    
    // When running in containers, services often bind on 0.0.0.0, but the cluster
    // topology may list 127.0.0.1. Normalize just for validation.
    let normalized_hostname_for_validation = if hostname == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        hostname.clone()
    };
    
    init_logging(&format!("output/logs/be_{}.log", rpc_port));
    log_startup("BE");
    log!("Starting the backend in {:?} deployment mode, version {}", deployment_mode, BUILD_VERSION);
    log!("RPC port: {}, HTTP port: {}", rpc_port, http_port);
    set_panic_hook();
    
    let deployment_mode_str = match deployment_mode {
        DeploymentMode::Aws => "aws",
        DeploymentMode::Localhost => "localhost",
    };
    
    // Store deployment mode globally
    crate::backend_config::set_deployment_mode(deployment_mode_str.to_string());
    
    // Initialize ClusterRegistry early
    let _registry = create_cluster_registry(deployment_mode_str);
    
    // Create DiscoveredTopology in AWS mode
    if deployment_mode == DeploymentMode::Aws {
        let discovered_topology = create_discovered_topology(&hostname, rpc_port).await;
        discovered_topology.log_self();
        start_periodic_discovery(Arc::new(Mutex::new(discovered_topology)));
        
        // Start HTTP server for debug endpoints (in both AWS and localhost modes)
        tokio::spawn(start_http_server(http_port));
    } else {
        // Start HTTP server in localhost mode as well
        tokio::spawn(start_http_server(http_port));
    }
    
    // Note: Topology validation is now done during InitColonyShard processing using routing table from coordinator
    // No static topology access needed at startup

    // Backend ticker will be started by coordinator via StartTicking RPC after colony initialization
    // Do NOT start ticker automatically here

    let bind_host = match deployment_mode {
        DeploymentMode::Aws => "0.0.0.0".to_string(),
        DeploymentMode::Localhost => hostname.clone(),
    };
    let bind_addr = format!("{}:{}", bind_host, rpc_port);
    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(listener) => listener,
        Err(err) => {
            log_error!("Failed to bind listener on {}: {}", bind_addr, err);
            panic!("Could not bind listener on {}: {}", bind_addr, err);
        }
    };
    log!("Listening on {} (advertised as {})", bind_addr, hostname);

    // Register backend in ClusterRegistry
    let (backend_private_ip, backend_public_ip, instance_id) = match deployment_mode {
        DeploymentMode::Aws => {
            // Get actual EC2 private IP
            let private_ip = match shared::utils::get_ec2_private_ip().await {
                Some(ip) => {
                    log!("Discovered EC2 private IP: {}", ip);
                    ip
                }
                None => {
                    log_error!("Failed to get EC2 private IP, registration will fail");
                    "0.0.0.0".to_string()
                }
            };
            // Get actual EC2 public IP
            let public_ip = match shared::utils::get_ec2_public_ip().await {
                Some(ip) => {
                    log!("Discovered EC2 public IP: {}", ip);
                    ip
                }
                None => {
                    log_error!("Failed to get EC2 public IP, registration will fail");
                    "0.0.0.0".to_string()
                }
            };
            let id = match shared::utils::get_ec2_instance_id().await {
                Some(id) => {
                    log!("Discovered EC2 instance ID: {}", id);
                    id
                }
                None => {
                    log_error!("Failed to get EC2 instance ID, using backend_{}", rpc_port);
                    format!("backend_{}", rpc_port)
                }
            };
            (private_ip, public_ip, id)
        }
        DeploymentMode::Localhost => (normalized_hostname_for_validation.clone(), normalized_hostname_for_validation.clone(), format!("backend_{}", rpc_port)),
    };
    // Use RPC port for internal communication and HTTP port for HTTP endpoints
    let backend_address = NodeAddress::new(backend_private_ip.clone(), backend_public_ip.clone(), rpc_port, http_port);
    let internal_addr = backend_address.to_internal_address();
    let http_addr = backend_address.to_http_address();
    if let Some(registry) = get_instance() {
        if let Err(e) = registry.register_backend(instance_id.clone(), backend_address).await {
            log_error!("Failed to register backend: {}", e);
        } else {
            log!("Registered backend {} in SSM ClusterRegistry: {} (internal), {} (http)", 
                 instance_id, internal_addr, http_addr);
        }
    }

    // Setup signal handlers for graceful shutdown
    let registry_clone = get_instance();
    let instance_id_clone = instance_id.clone();
    tokio::spawn(async move {
        use tokio::signal;
        let _ = signal::ctrl_c().await;
        log!("Received shutdown signal, unregistering backend...");
        if let Some(registry) = registry_clone {
            if let Err(e) = registry.unregister_backend(instance_id_clone).await {
                log_error!("Failed to unregister backend: {}", e);
            }
        }
        std::process::exit(0);
    });

    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("Connection failed: {}", e),
        }
    }
} 