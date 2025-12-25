mod init_colony;
mod global_topography;
mod coordinator_storage;
mod coordinator_context;
mod coordinator_ticker;
mod backend_client;
mod tick_monitor;
mod colony_event_generator;
mod colony_start;
mod http_server;
mod colony_capture;
mod colony_stats;
mod event_logging;

use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse, RoutingEntry};
use shared::cluster_topology::{ClusterTopology, NodeAddress};
use shared::cluster_registry::{ClusterRegistry, create_cluster_registry, get_instance};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error, log};
use bincode;
use futures_util::SinkExt;
use crate::http_server::start_http_server;


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

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

const BUILD_VERSION: &str = match option_env!("BUILD_VERSION") {
    Some(value) => value,
    None => "unknown",
};

fn call_label(response: &CoordinatorResponse) -> &'static str {
    match response {
        CoordinatorResponse::GetRoutingTableResponse { .. } => "GetRoutingTable",
    }
}

async fn send_response(framed: &mut FramedStream, response: CoordinatorResponse) {
    let encoded = bincode::serialize(&response).expect("Failed to serialize CoordinatorResponse");
    let label = call_label(&response);
    if let Err(e) = framed.send(encoded.into()).await {
        log_error!("Failed to send {} response: {}", label, e);
    } else {
        log!("Sent {} response", label);
    }
}

async fn handle_get_routing_table() -> CoordinatorResponse {
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized");
            return CoordinatorResponse::GetRoutingTableResponse { entries: Vec::new() };
        }
    };
    let mut entries = Vec::new();
    
    for shard in topology.get_all_shards() {
        let host_info = topology.get_host_for_shard(&shard).unwrap();
        entries.push(RoutingEntry {
            shard,
            hostname: host_info.hostname.clone(),
            port: host_info.port,
        });
    }

    CoordinatorResponse::GetRoutingTableResponse { entries }
}



async fn handle_client(socket: TcpStream) {
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        let response = match bincode::deserialize::<CoordinatorRequest>(&bytes) {
            Ok(CoordinatorRequest::GetRoutingTable) => handle_get_routing_table().await,
            Err(e) => {
                log_error!("Failed to deserialize CoordinatorRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
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
    eprintln!("COORDINATOR MAIN ENTERED");
    eprintln!("BUILD_VERSION={}", BUILD_VERSION);

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    eprintln!("Raw args = {:?}", args);
    
    // In AWS mode, get ports from environment variables if not provided as arguments
    let (rpc_port, http_port, deployment_mode) = if args.len() == 2 {
        // AWS mode: get from environment variables
        let deployment_mode = DeploymentMode::from_str(&args[1]).expect("Invalid deployment mode");
        if deployment_mode != DeploymentMode::Aws {
            eprintln!("Usage: {} <rpc_port> <http_port> <deployment_mode>", args[0]);
            eprintln!("Example: {} 8082 8083 localhost", args[0]);
            eprintln!("Deployment modes: localhost, aws");
            std::process::exit(1);
        }
        let rpc_env = std::env::var("RPC_PORT");
        let http_env = std::env::var("HTTP_PORT");
        let rpc_port = rpc_env
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .expect("RPC_PORT environment variable must be set in AWS mode");
        let http_port = http_env
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .expect("HTTP_PORT environment variable must be set in AWS mode");
        eprintln!("M1.2: parsed AWS ports rpc_port={}, http_port={}", rpc_port, http_port);
        (rpc_port, http_port, deployment_mode)
    } else if args.len() == 4 {
        // Localhost mode: get from command line arguments
        let rpc_port: u16 = args[1].parse().expect("RPC port must be a valid number");
        let http_port: u16 = args[2].parse().expect("HTTP port must be a valid number");
        let deployment_mode = DeploymentMode::from_str(&args[3]).expect("Invalid deployment mode");
        (rpc_port, http_port, deployment_mode)
    } else {
        eprintln!("Usage: {} <rpc_port> <http_port> <deployment_mode>", args[0]);
        eprintln!("Example: {} 8082 8083 localhost", args[0]);
        eprintln!("Deployment modes: localhost, aws");
        eprintln!("In AWS mode, RPC_PORT and HTTP_PORT environment variables are used");
        std::process::exit(1);
    };
    let deployment_mode_str = match deployment_mode {
        DeploymentMode::Aws => "aws",
        DeploymentMode::Localhost => "localhost",
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
    let log_path = format!("output/logs/coordinator_{}.log", rpc_port);
    init_logging(&log_path);
    log_startup("COORDINATOR");

    log!("Starting coordinator in {:?} deployment mode, version {}", deployment_mode, BUILD_VERSION);
    log!("RPC port: {}, HTTP port: {}", rpc_port, http_port);
    set_panic_hook();
    
    // Initialize ClusterRegistry early
    let _registry = create_cluster_registry(deployment_mode_str);
    
    // Store deployment mode in coordinator context
    let context = crate::coordinator_context::CoordinatorContext::get_instance();
    context.set_deployment_mode(deployment_mode_str.to_string());
    
    // Coordinator ticker will be started by start_colony_ticking() after colony initialization
    // Do NOT start ticker automatically here
    
    // Topology is never initialized automatically - it must be created explicitly via POST /colony-start
    // This applies to both localhost and AWS modes
    log!("Waiting for colony-start HTTP request to initialize topology and colony");

    // Start HTTP server (in both AWS and localhost modes)
    tokio::spawn(start_http_server(http_port));

    // Start TCP listener for coordinator protocol
    let bind_host = match deployment_mode {
        DeploymentMode::Aws => "0.0.0.0",
        DeploymentMode::Localhost => "127.0.0.1",
    };
    let addr = format!("{}:{}", bind_host, rpc_port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(err) => {
            log_error!("Failed to bind coordinator protocol listener on {}: {}", addr, err);
            panic!("Could not bind coordinator protocol listener on {}: {}", addr, err);
        }
    };
    log!("Listening on {} for coordinator protocol", addr);

    // Register coordinator in ClusterRegistry
    let (coordinator_private_ip, coordinator_public_ip) = match deployment_mode {
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
            (private_ip, public_ip)
        }
        DeploymentMode::Localhost => ("127.0.0.1".to_string(), "127.0.0.1".to_string()),
    };
    // Use RPC port for internal communication and HTTP port for HTTP endpoints
    let coordinator_address = NodeAddress::new(coordinator_private_ip.clone(), coordinator_public_ip.clone(), rpc_port, http_port);
    let internal_addr = coordinator_address.to_internal_address();
    let http_addr = coordinator_address.to_http_address();
    if let Some(registry) = get_instance() {
        if let Err(e) = registry.register_coordinator(coordinator_address).await {
            log_error!("Failed to register coordinator: {}", e);
        } else {
            log!("Registered coordinator in SSM ClusterRegistry: {} (internal), {} (http)", 
                 internal_addr, http_addr);
        }
    }

    // Setup signal handlers for graceful shutdown
    let registry_clone = get_instance();
    tokio::spawn(async move {
        use tokio::signal;
        let _ = signal::ctrl_c().await;
        log!("Received shutdown signal, unregistering coordinator...");
        if let Some(registry) = registry_clone {
            if let Err(e) = registry.unregister_coordinator().await {
                log_error!("Failed to unregister coordinator: {}", e);
            }
        }
        std::process::exit(0);
    });

    // Start periodic creature image capture task (runs every 60 seconds)
    tokio::spawn(async move {
        use tokio::time::{interval, Duration};
        let mut capture_interval = interval(Duration::from_secs(60));
        // Skip the first tick which fires immediately, then start capturing
        capture_interval.tick().await;
        
        loop {
            capture_interval.tick().await;
            crate::colony_capture::capture_colony().await;
        }
    });

    tokio::spawn(async move {
        use tokio::time::{interval, Duration};
        let mut stats_interval = interval(Duration::from_secs(10));
        stats_interval.tick().await;
        
        loop {
            stats_interval.tick().await;
            crate::colony_stats::capture_colony_stats().await;
        }
    });

    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                log!("Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("Connection failed: {}", e),
        }
    }
} 