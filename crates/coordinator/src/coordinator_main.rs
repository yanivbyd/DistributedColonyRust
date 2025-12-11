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

use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse, RoutingEntry, ColonyMetricStats};
use std::collections::{BTreeMap, HashMap};
use shared::cluster_topology::{ClusterTopology, NodeAddress};
use shared::cluster_registry::{ClusterRegistry, create_cluster_registry, get_instance};
use shared::be_api::StatMetric;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error, log};
use bincode;
use futures_util::SinkExt;
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

use crate::coordinator_context::CoordinatorContext;
use crate::http_server::start_http_server;

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

const BUILD_VERSION: &str = match option_env!("BUILD_VERSION") {
    Some(value) => value,
    None => "unknown",
};

fn call_label(response: &CoordinatorResponse) -> &'static str {
    match response {
        CoordinatorResponse::GetRoutingTableResponse { .. } => "GetRoutingTable",
        CoordinatorResponse::GetColonyEventsResponse { .. } => "GetColonyEvents",
        CoordinatorResponse::GetColonyStatsResponse { .. } => "GetColonyStats",
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

async fn handle_get_colony_events(limit: usize) -> CoordinatorResponse {
    let context = CoordinatorContext::get_instance();
    let mut events = context.get_colony_events();
    
    // Sort by tick in descending order (most recent first)
    events.sort_by(|a, b| b.tick.cmp(&a.tick));
    
    // Take only the top K events
    let limited_events = events.into_iter().take(limit).collect();
    
    CoordinatorResponse::GetColonyEventsResponse { 
        events: limited_events
    }
}


async fn handle_client(socket: TcpStream) {
    log!("handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        let response = match bincode::deserialize::<CoordinatorRequest>(&bytes) {
            Ok(CoordinatorRequest::GetRoutingTable) => handle_get_routing_table().await,
            Ok(CoordinatorRequest::GetColonyEvents { limit }) => handle_get_colony_events(limit).await,
            Ok(CoordinatorRequest::GetColonyStats { metrics }) => handle_get_colony_stats(metrics).await,
            Err(e) => {
                log_error!("Failed to deserialize CoordinatorRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
    log!("handle_client: connection closed");
}

async fn handle_get_colony_stats(metrics: Vec<StatMetric>) -> CoordinatorResponse {
    // Aggregate across all shards
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized");
            return CoordinatorResponse::GetColonyStatsResponse { metrics: Vec::new(), tick_count: 0 };
        }
    };
    let shards = topology.get_all_shards();
    if shards.is_empty() {
        return CoordinatorResponse::GetColonyStatsResponse { metrics: Vec::new(), tick_count: 0 };
    }

    // Prepare index mapping for requested metrics
    fn metric_id(m: shared::be_api::StatMetric) -> u8 {
        match m {
            shared::be_api::StatMetric::Health => 0,
            shared::be_api::StatMetric::CreatureSize => 1,
            shared::be_api::StatMetric::CreateCanKill => 2,
            shared::be_api::StatMetric::CreateCanMove => 3,
            shared::be_api::StatMetric::Food => 4,
            shared::be_api::StatMetric::Age => 5,
        }
    }
    let mut pos_by_id: HashMap<u8, usize> = HashMap::new();
    for (idx, m) in metrics.iter().copied().enumerate() {
        pos_by_id.insert(metric_id(m), idx);
    }
    // counts_per_metric: per requested metric (by index) -> value -> occs
    let mut counts_per_metric: Vec<BTreeMap<i32, u64>> = vec![BTreeMap::new(); metrics.len()];

    let mut min_tick: Option<u64> = None;
    for shard in shards {
        if let Some((tick, per_metric)) = crate::backend_client::call_backend_get_shard_stats(shard, metrics.clone()) {
            min_tick = Some(match min_tick { Some(t) => t.min(tick), None => tick });
            for (metric, buckets) in per_metric {
                if let Some(&idx) = pos_by_id.get(&metric_id(metric)) {
                    let entry = counts_per_metric.get_mut(idx).unwrap();
                    for b in buckets {
                        *entry.entry(b.value).or_insert(0) += b.occs;
                    }
                }
            }
        }
    }

    // Build ordered results following the requested metrics order
    let mut results: Vec<ColonyMetricStats> = Vec::with_capacity(metrics.len());
    for (i, metric) in metrics.into_iter().enumerate() {
        let counts = std::mem::take(&mut counts_per_metric[i]);
        let mut sum: i64 = 0;
        let mut total: i64 = 0;
        for (value, occs) in &counts {
            sum += *value as i64 * *occs as i64;
            total += *occs as i64;
        }
        let avg = if total > 0 { sum as f64 / total as f64 } else { 0.0 };
        let buckets = counts.into_iter().map(|(value, occs)| shared::be_api::StatBucket { value, occs }).collect();
        results.push(ColonyMetricStats { metric, avg, buckets });
    }

    CoordinatorResponse::GetColonyStatsResponse { metrics: results, tick_count: min_tick.unwrap_or(0) }
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
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    
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
        let rpc_port = std::env::var("RPC_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .expect("RPC_PORT environment variable must be set in AWS mode");
        let http_port = std::env::var("HTTP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .expect("HTTP_PORT environment variable must be set in AWS mode");
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
    
    init_logging(&format!("output/logs/coordinator_{}.log", rpc_port));
    log_startup("COORDINATOR");
    log!("Starting coordinator in {:?} deployment mode, version {}", deployment_mode, BUILD_VERSION);
    log!("RPC port: {}, HTTP port: {}", rpc_port, http_port);
    set_panic_hook();
    
    // Initialize ClusterRegistry early
    let _registry = create_cluster_registry(deployment_mode_str);
    
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
    let coordinator_ip = match deployment_mode {
        DeploymentMode::Aws => "0.0.0.0", // Will be replaced with actual IP in AWS
        DeploymentMode::Localhost => "127.0.0.1",
    };
    // Use RPC port for internal communication and HTTP port for HTTP endpoints
    let coordinator_address = NodeAddress::new(coordinator_ip.to_string(), rpc_port, http_port);
    if let Some(registry) = get_instance() {
        if let Err(e) = registry.register_coordinator(coordinator_address).await {
            log_error!("Failed to register coordinator: {}", e);
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

    loop {
        log!("Waiting for connection...");
        match listener.accept().await {
            Ok((socket, _)) => {
                log!("Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("Connection failed: {}", e),
        }
    }
} 