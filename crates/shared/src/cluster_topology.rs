use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};
use crate::log;

// Configuration constants
const COORDINATOR_PORT: u16 = 8082;
const BACKEND_PORTS: &[u16] = &[8084, 8086, 8088, 8090];
const HOSTNAME: &str = "127.0.0.1";
const WIDTH_IN_SHARDS: i32 = 8;
const HEIGHT_IN_SHARDS: i32 = 5;
const SHARD_WIDTH: i32 = 250;
const SHARD_HEIGHT: i32 = 250;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    Coordinator,
    Backend,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeStatus {
    Active,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeAddress {
    pub ip: String,
    pub internal_port: u16,
    pub http_port: u16,
}

impl NodeAddress {
    pub fn new(ip: String, internal_port: u16, http_port: u16) -> Self {
        Self { ip, internal_port, http_port }
    }

    pub fn to_address(&self) -> String {
        format!("{}:{}", self.ip, self.internal_port)
    }

    pub fn to_internal_address(&self) -> String {
        format!("{}:{}", self.ip, self.internal_port)
    }

    pub fn to_http_address(&self) -> String {
        format!("{}:{}", self.ip, self.http_port)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_type: NodeType,
    pub address: NodeAddress,
    pub status: NodeStatus,
}

impl NodeInfo {
    pub fn new(node_type: NodeType, address: NodeAddress, status: NodeStatus) -> Self {
        Self { node_type, address, status }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredTopology {
    pub self_type: NodeType,
    pub self_address: NodeAddress,
    pub coordinator_info: Option<NodeInfo>,
    pub backend_info: Vec<NodeInfo>,
}

impl DiscoveredTopology {
    pub fn new(
        self_type: NodeType,
        self_address: NodeAddress,
        coordinator_info: Option<NodeInfo>,
        backend_info: Vec<NodeInfo>,
    ) -> Self {
        Self {
            self_type,
            self_address,
            coordinator_info,
            backend_info,
        }
    }
    
    pub fn log_self(&self) {
        let coordinator_str = self.coordinator_info
            .as_ref()
            .map(|info| info.address.to_address())
            .unwrap_or_else(|| "None".to_string());
        log!("DiscoveredTopology: self={}, coordinator={}, backends={}", 
             self.self_address.to_address(),
             coordinator_str,
             self.backend_info.len());
    }
    
    pub async fn start_discovery(&mut self) {        
        log!("Starting topology discovery from AWS SSM...");
        
        // Discover coordinator
        if let Some(coordinator_address) = Self::discover_coordinator().await {
            let status = Self::check_node_status(&coordinator_address, NodeType::Coordinator).await;
            self.coordinator_info = Some(NodeInfo::new(
                NodeType::Coordinator,
                coordinator_address,
                status,
            ));
            log!("Discovered coordinator: {:?}", self.coordinator_info);
        } else {
            log!("No coordinator found in SSM");
        }
        
        // Discover backends
        let backend_addresses = Self::discover_backends().await;
        log!("Discovered {} backends from SSM", backend_addresses.len());
        
        for address in backend_addresses {
            let status = Self::check_node_status(&address, NodeType::Backend).await;
            let node_info = NodeInfo::new(NodeType::Backend, address, status);
            self.backend_info.push(node_info);
        }
        
        log!("Topology discovery complete: coordinator={}, backends={}", 
             self.coordinator_info.is_some(), 
             self.backend_info.len());
    }
    
    pub async fn refresh_topology(&mut self) {
        // Discover coordinator
        let new_coordinator = Self::discover_coordinator().await;
        let coordinator_changed = match (&self.coordinator_info, &new_coordinator) {
            (Some(old), Some(new_addr)) => old.address.to_address() != new_addr.to_address(),
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (None, None) => false,
        };
        
        if coordinator_changed {
            if let Some(coordinator_address) = new_coordinator {
                let status = Self::check_node_status(&coordinator_address, NodeType::Coordinator).await;
                self.coordinator_info = Some(NodeInfo::new(
                    NodeType::Coordinator,
                    coordinator_address.clone(),
                    status,
                ));
                log!("Coordinator changed: {}", coordinator_address.to_address());
            } else {
                self.coordinator_info = None;
                log!("Coordinator removed from topology");
            }
        } else if let Some(coordinator_info) = &mut self.coordinator_info {
            // Update status for existing coordinator
            coordinator_info.status = Self::check_node_status(&coordinator_info.address, NodeType::Coordinator).await;
        }
        
        // Discover backends
        let new_backend_addresses = Self::discover_backends().await;
        let old_backend_addresses: Vec<String> = self.backend_info
            .iter()
            .map(|info| info.address.to_address())
            .collect();
        let new_backend_addresses_str: Vec<String> = new_backend_addresses
            .iter()
            .map(|addr| addr.to_address())
            .collect();
        
        // Check for added backends
        for address in &new_backend_addresses {
            let addr_str = address.to_address();
            if !old_backend_addresses.contains(&addr_str) {
                let status = Self::check_node_status(address, NodeType::Backend).await;
                let node_info = NodeInfo::new(NodeType::Backend, address.clone(), status);
                self.backend_info.push(node_info);
                log!("New backend added: {}", addr_str);
            }
        }
        
        // Check for removed backends
        self.backend_info.retain(|info| {
            let addr_str = info.address.to_address();
            let retained = new_backend_addresses_str.contains(&addr_str);
            if !retained {
                log!("Backend removed: {}", addr_str);
            }
            retained
        });
        
        // Update status for existing backends
        for backend in &mut self.backend_info {
            backend.status = Self::check_node_status(&backend.address, NodeType::Backend).await;
        }
    }
    
    async fn discover_coordinator() -> Option<NodeAddress> {
        crate::ssm::discover_coordinator().await
    }
    
    async fn discover_backends() -> Vec<NodeAddress> {
        crate::ssm::discover_backends().await
    }
    
    async fn check_node_status(address: &NodeAddress, node_type: NodeType) -> NodeStatus {
        use tokio::time::{timeout, Duration};
        use tokio::net::TcpStream;
        use tokio_util::codec::{Framed, LengthDelimitedCodec};
        use futures_util::SinkExt;
        use tokio_stream::StreamExt;
        use crate::be_api::{BackendRequest, BackendResponse};
        use crate::coordinator_api::{CoordinatorRequest, CoordinatorResponse};
        
        let addr = address.to_address();
        let connect_timeout = Duration::from_secs(2);
        
        match timeout(connect_timeout, TcpStream::connect(&addr)).await {
            Ok(Ok(stream)) => {
                let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
                
                match node_type {
                    NodeType::Backend => {
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
                    }
                    NodeType::Coordinator => {
                        // Send GetRoutingTable request to coordinator
                        let routing_request = CoordinatorRequest::GetRoutingTable;
                        if let Ok(encoded) = bincode::serialize(&routing_request) {
                            if framed.send(encoded.into()).await.is_ok() {
                                // Wait for response
                                let response_timeout = Duration::from_secs(2);
                                match timeout(response_timeout, framed.next()).await {
                                    Ok(Some(Ok(bytes))) => {
                                        if let Ok(CoordinatorResponse::GetRoutingTableResponse { .. }) = 
                                            bincode::deserialize::<CoordinatorResponse>(&bytes) {
                                            return NodeStatus::Active;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                NodeStatus::Unknown
            }
            _ => NodeStatus::Unknown,
        }
    }
}

use tokio::sync::Mutex;

pub fn start_periodic_discovery(topology: Arc<Mutex<DiscoveredTopology>>) {
    tokio::spawn(async move {
        use tokio::time::{interval, Duration};
        
        let mut timer = interval(Duration::from_secs(10));
        // Skip the first tick which fires immediately
        timer.tick().await;
        
        log!("Starting periodic topology discovery (every 10 seconds)");
        
        loop {
            timer.tick().await;
            
            log!("Running periodic topology refresh...");
            let mut topology_guard = topology.lock().await;
            topology_guard.refresh_topology().await;
            drop(topology_guard);
        }
    });
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub hostname: String,
    pub port: u16,
}

impl HostInfo {
    pub fn new(hostname: String, port: u16) -> Self {
        Self { hostname, port }
    }

    pub fn to_address(&self) -> String {
        format!("{}:{}", self.hostname, self.port)
    }
}

impl PartialEq for HostInfo {
    fn eq(&self, other: &Self) -> bool {
        self.hostname == other.hostname && self.port == other.port
    }
}

impl Eq for HostInfo {}

impl std::hash::Hash for HostInfo {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hostname.hash(state);
        self.port.hash(state);
    }
}

pub struct ClusterTopology {
    coordinator_host: HostInfo,
    backend_hosts: Vec<HostInfo>,
    shard_to_host: HashMap<Shard, HostInfo>
}

static INSTANCE: OnceLock<RwLock<Arc<ClusterTopology>>> = OnceLock::new();

impl ClusterTopology {
    fn topology_lock() -> &'static RwLock<Arc<ClusterTopology>> {
        INSTANCE.get_or_init(|| RwLock::new(Arc::new(Self::new_fixed_topology())))
    }

    pub fn get_instance() -> Arc<ClusterTopology> {
        Self::topology_lock().read().expect("ClusterTopology lock poisoned").clone()
    }
    
    /// Check if ClusterTopology has been initialized
    pub fn is_initialized() -> bool {
        INSTANCE.get().is_some()
    }
    
    /// Initialize ClusterTopology with a dynamic topology (for cloud-start mode)
    /// This must be called before get_instance() is called for the first time
    /// Returns Ok(()) if successful
    pub fn initialize_with_dynamic_topology(backend_hosts: Vec<HostInfo>, shard_to_host: HashMap<Shard, HostInfo>) -> Result<(), String> {
        let coordinator_host = HostInfo::new(HOSTNAME.to_string(), COORDINATOR_PORT);
        let topology = ClusterTopology {
            coordinator_host,
            backend_hosts,
            shard_to_host,
        };
        let topology = Arc::new(topology);
        match Self::topology_lock().write() {
            Ok(mut guard) => {
                *guard = topology;
                Ok(())
            }
            Err(_) => Err("ClusterTopology lock poisoned".to_string()),
        }
    }
    
    /// Get the configured backend ports
    pub fn get_backend_ports() -> &'static [u16] {
        BACKEND_PORTS
    }
    
    /// Get the coordinator port
    pub fn get_coordinator_port() -> u16 {
        COORDINATOR_PORT
    }
    
    /// Get the hostname used for all services
    pub fn get_hostname() -> &'static str {
        HOSTNAME
    }
    
    /// Get the grid width in shards
    pub fn get_width_in_shards() -> i32 {
        WIDTH_IN_SHARDS
    }
    
    /// Get the grid height in shards
    pub fn get_height_in_shards() -> i32 {
        HEIGHT_IN_SHARDS
    }
    
    /// Get the individual shard width
    pub fn get_shard_width() -> i32 {
        SHARD_WIDTH
    }
    
    /// Get the individual shard height
    pub fn get_shard_height() -> i32 {
        SHARD_HEIGHT
    }
    
    fn new_fixed_topology() -> Self {
        let coordinator_host = HostInfo::new(HOSTNAME.to_string(), COORDINATOR_PORT);
        
        // Create backend hosts from the configured ports
        let backend_hosts: Vec<HostInfo> = BACKEND_PORTS.iter()
            .map(|&port| HostInfo::new(HOSTNAME.to_string(), port))
            .collect();
        
        let shards = Self::create_fixed_shards();
        let mut shard_to_host = HashMap::new();
        
        // Distribute shards evenly across all backends
        for (index, shard) in shards.iter().enumerate() {
            let backend_index = index % backend_hosts.len();
            let host = backend_hosts[backend_index].clone();
            shard_to_host.insert(*shard, host);
        }
        
        Self {
            coordinator_host,
            backend_hosts,
            shard_to_host
        }
    }
    
    fn create_fixed_shards() -> Vec<Shard> {
        let mut shards = Vec::new();
        
        // Create grid of shards using configured dimensions
        for y in 0..HEIGHT_IN_SHARDS {
            for x in 0..WIDTH_IN_SHARDS {
                let shard = Shard {
                    x: x * SHARD_WIDTH,
                    y: y * SHARD_HEIGHT,
                    width: SHARD_WIDTH,
                    height: SHARD_HEIGHT,
                };
                shards.push(shard);
            }
        }
        
        shards
    }
    
    pub fn get_all_backend_hosts(&self) -> &Vec<HostInfo> {
        &self.backend_hosts
    }
    
    pub fn get_coordinator_host(&self) -> &HostInfo {
        &self.coordinator_host
    }
    
    pub fn get_backend_hosts_for_shards(&self, shards: &[Shard]) -> HashSet<HostInfo> {
        let mut hosts = HashSet::new();
        
        for shard in shards {
            if let Some(host) = self.shard_to_host.get(shard) {
                hosts.insert(host.clone());
            }
        }
        
        hosts
    }
    
    pub fn get_host_for_shard(&self, shard: &Shard) -> Option<&HostInfo> {
        self.shard_to_host.get(shard)
    }
    
    pub fn get_all_shards(&self) -> Vec<Shard> {
        self.shard_to_host.keys().cloned().collect()
    }
    
    /// Check if a shard exists in the topology
    pub fn has_shard(&self, shard: &Shard) -> bool {
        self.shard_to_host.contains_key(shard)
    }
    
    /// Get the total number of shards
    pub fn shard_count(&self) -> usize {
        self.shard_to_host.len()
    }
    
    /// Get the total number of backend hosts
    pub fn backend_host_count(&self) -> usize {
        self.backend_hosts.len()
    }
    
    /// Get all shards that are adjacent to the given shard
    pub fn get_adjacent_shards(&self, shard: &Shard) -> Vec<Shard> {
        let mut adjacent_shards = Vec::new();
        
        for other_shard in self.get_all_shards() {
            if self.are_shards_adjacent(shard, &other_shard) {
                adjacent_shards.push(other_shard);
            }
        }
        
        adjacent_shards
    }
    
    fn are_shards_adjacent(&self, shard1: &Shard, shard2: &Shard) -> bool {
        // Shards are adjacent if they share an edge
        let left1 = shard1.x;
        let right1 = shard1.x + shard1.width;
        let top1 = shard1.y;
        let bottom1 = shard1.y + shard1.height;
        
        let left2 = shard2.x;
        let right2 = shard2.x + shard2.width;
        let top2 = shard2.y;
        let bottom2 = shard2.y + shard2.height;
        
        // Check for horizontal adjacency (left-right)
        let horizontally_adjacent = (right1 == left2) || (right2 == left1);
        
        // Check for vertical adjacency (top-bottom)
        let vertically_adjacent = (bottom1 == top2) || (bottom2 == top1);
        
        // Check for corner adjacency (diagonal)
        let corner_adjacent = (right1 == left2 && (bottom1 == top2 || top1 == bottom2)) ||
                             (left1 == right2 && (bottom1 == top2 || top1 == bottom2));
        
        horizontally_adjacent || vertically_adjacent || corner_adjacent
    }
}

