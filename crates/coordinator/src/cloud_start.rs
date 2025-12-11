use shared::cluster_topology::{ClusterTopology, HostInfo, NodeAddress, NodeStatus, DiscoveredTopology, NodeType};
use shared::{log, log_error};
use shared::colony_model::Shard;
use std::collections::HashMap;
use crate::init_colony::initialize_colony;
use crate::coordinator_context::CoordinatorContext;

pub async fn cloud_start_colony(idempotency_key: Option<String>) {
    log!("Starting colony-start process: discovering backends and creating shard map");
    
    // Step 1: Discover available backend nodes from AWS config
    let available_backends = discover_and_ping_backends().await;
    
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
    
    // Step 3: Initialize ClusterTopology with dynamic topology
    // Note: This will fail if topology is already initialized (e.g., from normal startup)
    match ClusterTopology::initialize_with_dynamic_topology(available_backends, shard_map) {
        Ok(()) => {
            log!("ClusterTopology initialized with dynamic topology");
        }
        Err(err) => {
            log_error!("Failed to install dynamic topology: {}", err);
            return;
        }
    }
    
    // Step 4: Initialize and start the colony
    // Note: coordinator_ticker should already be started in main()
    initialize_colony().await;
    
    // Step 5: Store idempotency_key in memory after successful initialization
    if let Some(key) = idempotency_key {
        let context = CoordinatorContext::get_instance();
        let mut stored_info = context.get_coord_stored_info();
        stored_info.cloud_start_idempotency_key = Some(key.clone());
    }
    
    log!("Colony-start completed successfully");
}

async fn discover_and_ping_backends() -> Vec<HostInfo> {
    // Get RPC and HTTP ports from environment variables (AWS mode) or use defaults
    let rpc_port = std::env::var("RPC_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8082); // Default fallback
    let http_port = std::env::var("HTTP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8083); // Default fallback
    let mut discovered_topology = DiscoveredTopology::new(
        NodeType::Coordinator,
        NodeAddress::new("127.0.0.1".to_string(), rpc_port, http_port),
        None,
        Vec::new(),
    );
    
    // Discover backends from AWS SSM
    discovered_topology.start_discovery().await;
    
    // Get coordinator's own IP address to exclude it from backend list
    let coordinator_ip = discovered_topology.coordinator_info
        .as_ref()
        .map(|info| info.address.ip.clone());
    
    // Filter to only active backends, excluding the coordinator's own IP
    let mut available_backends = Vec::new();
    for backend_info in discovered_topology.backend_info {
        // Skip if this backend matches the coordinator's IP (coordinator should not be a backend)
        if let Some(ref coord_ip) = coordinator_ip {
            if backend_info.address.ip == *coord_ip {
                log_error!("Skipping backend {}:{} (matches coordinator IP)", backend_info.address.ip, backend_info.address.internal_port);
                continue;
            }
        }
        
        if backend_info.status == NodeStatus::Active {
            available_backends.push(HostInfo::new(
                backend_info.address.ip,
                backend_info.address.internal_port,
            ));
        } else {
            log!("Skipping backend {}:{} (status: {:?})", 
                 backend_info.address.ip, backend_info.address.internal_port, backend_info.status);
        }
    }
    
    available_backends
}

fn create_shard_map_with_even_distribution(backends: &[HostInfo]) -> HashMap<Shard, HostInfo> {
    // Get shard configuration from ClusterTopology constants
    let width_in_shards = ClusterTopology::get_width_in_shards();
    let height_in_shards = ClusterTopology::get_height_in_shards();
    let shard_width = ClusterTopology::get_shard_width();
    let shard_height = ClusterTopology::get_shard_height();
    
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

