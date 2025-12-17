use shared::cluster_topology::{ClusterTopology, HostInfo, NodeAddress, NodeStatus, TopologyConfig};
use shared::{log, log_error};
use shared::colony_model::Shard;
use shared::cluster_registry::{get_instance, ClusterRegistry};
use std::collections::HashMap;
use crate::init_colony::initialize_colony;
use crate::coordinator_context::CoordinatorContext;

pub async fn colony_start_colony(idempotency_key: Option<String>) {
    log!("Starting colony-start process: discovering backends and creating shard map");
    
    // Step 1: Discover available backend nodes from AWS config
    let (available_backends, coordinator_address) = discover_and_ping_backends().await;
    
    if available_backends.is_empty() {
        log_error!("No available backend nodes found. Cannot start colony.");
        return;
    }
    
    log!("Found {} available backend nodes", available_backends.len());
    for backend in &available_backends {
        log!("  - {}:{}", backend.hostname, backend.port);
    }
    
    // Step 2: Create shard map based on available nodes with even distribution
    let shard_map = create_shard_map_with_even_distribution(&available_backends);
    
    log!("Created shard map with {} shards distributed across {} backends", 
         shard_map.len(), available_backends.len());
    
    // Step 3: Get coordinator host info from discovered topology (use self_address since coordinator is discovering itself)
    let coordinator_host = HostInfo::new(
        coordinator_address.private_ip.clone(),
        coordinator_address.internal_port
    );
    
    // Step 4: Store shard dimensions in coordinator context (in memory only)
    let context = CoordinatorContext::get_instance();
    let deployment_mode = context.get_deployment_mode()
        .unwrap_or_else(|| "localhost".to_string());
    let width_in_shards = ClusterTopology::width_in_shards_for_mode(&deployment_mode);
    let height_in_shards = ClusterTopology::height_in_shards_for_mode(&deployment_mode);
    let shard_width = ClusterTopology::default_shard_width();
    let shard_height = ClusterTopology::default_shard_height();
    
    // Store dimensions in context (these can be derived from topology later, but storing for convenience)
    {
        let mut stored_info = context.get_coord_stored_info();
        stored_info.colony_width = Some(width_in_shards * shard_width);
        stored_info.colony_height = Some(height_in_shards * shard_height);
    } // Drop mutex guard before await
    
    // Step 5: Initialize ClusterTopology with dynamic topology
    let config = TopologyConfig::new(coordinator_host, available_backends, shard_map);
    match ClusterTopology::initialize(config) {
        Ok(_) => {
            log!("ClusterTopology initialized with dynamic topology");
        }
        Err(err) => {
            log_error!("Failed to install dynamic topology: {}", err);
            return;
        }
    }
    
    // Step 6: Initialize and start the colony
    // Note: coordinator_ticker should already be started in main()
    initialize_colony().await;
    
    // Step 7: Store idempotency_key in memory after successful initialization
    if let Some(key) = idempotency_key {
        let mut stored_info = context.get_coord_stored_info();
        stored_info.colony_start_idempotency_key = Some(key.clone());
    }
    
    log!("Colony-start completed successfully");
}

async fn discover_and_ping_backends() -> (Vec<HostInfo>, NodeAddress) {
    // Get coordinator address from ClusterRegistry
    let registry = match get_instance() {
        Some(r) => r,
        None => {
            log_error!("ClusterRegistry not initialized");
            return (Vec::new(), NodeAddress::new("127.0.0.1".to_string(), "127.0.0.1".to_string(), 8082, 8083));
        }
    };
    
    // Get coordinator address (for localhost, this should be registered; for AWS, discover from SSM)
    let coordinator_address = match registry.discover_coordinator().await {
        Some(addr) => addr,
        None => {
            // Fallback: try to get from environment or use defaults
            let rpc_port = std::env::var("RPC_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(8082);
            let http_port = std::env::var("HTTP_PORT")
                .ok()
                .and_then(|v| v.parse::<u16>().ok())
                .unwrap_or(8083);
            NodeAddress::new("127.0.0.1".to_string(), "127.0.0.1".to_string(), rpc_port, http_port)
        }
    };
    
    // Discover backends from ClusterRegistry (works for both localhost and AWS)
    let backend_addresses = registry.discover_backends().await;
    log!("Discovered {} backends from ClusterRegistry", backend_addresses.len());
    
    // Filter backends, excluding the coordinator
    let filtered_backends = filter_backends_excluding_coordinator(backend_addresses, &coordinator_address).await;
    
    (filtered_backends, coordinator_address)
}

/// Filter backend addresses, excluding the coordinator and inactive backends.
/// This function is extracted for testability.
/// 
/// In localhost mode, coordinator and backends share the same IP (127.0.0.1),
/// so we must compare both IP and port to correctly exclude the coordinator.
pub(crate) async fn filter_backends_excluding_coordinator(
    backend_addresses: Vec<NodeAddress>,
    coordinator_address: &NodeAddress,
) -> Vec<HostInfo> {
    let coordinator_internal_port = coordinator_address.internal_port;
    
    // Filter to only active backends, excluding the coordinator itself
    let mut available_backends = Vec::new();
    for backend_address in backend_addresses {
        // Skip if this backend matches the coordinator's address (same IP and port)
        // In localhost mode, IPs will match, so we check the port
        if backend_address.private_ip == coordinator_address.private_ip && 
           backend_address.internal_port == coordinator_internal_port {
            log!("Skipping backend {}:{} (matches coordinator address)", backend_address.private_ip, backend_address.internal_port);
            continue;
        }
        
        // Check if backend is active by attempting to connect
        let status = check_backend_status(&backend_address).await;
        if status == NodeStatus::Active {
            available_backends.push(HostInfo::new(
                backend_address.private_ip,
                backend_address.internal_port,
            ));
        } else {
            log!("Skipping backend {}:{} (status: {:?})", 
                 backend_address.private_ip, backend_address.internal_port, status);
        }
    }
    
    available_backends
}

async fn check_backend_status(address: &NodeAddress) -> NodeStatus {
    use tokio::time::{timeout, Duration};
    use tokio::net::TcpStream;
    use tokio_util::codec::{Framed, LengthDelimitedCodec};
    use futures_util::SinkExt;
    use tokio_stream::StreamExt;
    use shared::be_api::{BackendRequest, BackendResponse};
    
    let addr = address.to_address();
    let connect_timeout = Duration::from_secs(2);
    
    match timeout(connect_timeout, TcpStream::connect(&addr)).await {
        Ok(Ok(stream)) => {
            let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
            
            // Send ping request to backend
            let ping_request = BackendRequest::Ping;
            if let Ok(encoded) = bincode::serialize(&ping_request) {
                if framed.send(encoded.into()).await.is_ok() {
                    // Wait for response
                    let response_timeout = Duration::from_secs(2);
                    match timeout(response_timeout, framed.next()).await {
                        Ok(Some(Ok(bytes))) => {
                            if let Ok(BackendResponse::Ping) = bincode::deserialize::<BackendResponse>(&bytes) {
                                return NodeStatus::Active;
                            }
                        }
                        _ => {}
                    }
                }
            }
            NodeStatus::Unknown
        }
        _ => NodeStatus::Unknown,
    }
}

fn create_shard_map_with_even_distribution(backends: &[HostInfo]) -> HashMap<Shard, HostInfo> {
    // Get shard configuration based on deployment mode
    let context = CoordinatorContext::get_instance();
    let deployment_mode = context.get_deployment_mode()
        .unwrap_or_else(|| "localhost".to_string());
    let width_in_shards = ClusterTopology::width_in_shards_for_mode(&deployment_mode);
    let height_in_shards = ClusterTopology::height_in_shards_for_mode(&deployment_mode);
    let shard_width = ClusterTopology::default_shard_width();
    let shard_height = ClusterTopology::default_shard_height();
    
    // Create all shards
    let mut shards = Vec::new();
    for y in 0..height_in_shards {
        for x in 0..width_in_shards {
            let shard = Shard {
                x: x * shard_width,
                y: y * shard_height,
                width: shard_width,
                height: shard_height,
            };
            shards.push(shard);
        }
    }
    
    // Distribute shards evenly across backends using round-robin
    let mut shard_map = HashMap::new();
    for (index, shard) in shards.iter().enumerate() {
        let backend_index = index % backends.len();
        let host = backends[backend_index].clone();
        shard_map.insert(*shard, host);
    }
    
    // Log distribution
    let mut shards_per_backend: HashMap<String, usize> = HashMap::new();
    for host in shard_map.values() {
        let key = host.to_address();
        *shards_per_backend.entry(key).or_insert(0) += 1;
    }
    
    for (backend, count) in shards_per_backend {
        log!("  {}: {} shards", backend, count);
    }
    
    shard_map
}
