use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};
use crate::log;

// Default shard dimensions for first initialization
const DEFAULT_WIDTH_IN_SHARDS: i32 = 5;
const DEFAULT_HEIGHT_IN_SHARDS: i32 = 4;
const DEFAULT_SHARD_WIDTH: i32 = 250;
const DEFAULT_SHARD_HEIGHT: i32 = 250;

/// Configuration for initializing topology
#[derive(Debug, Clone)]
pub struct TopologyConfig {
    pub coordinator_host: HostInfo,
    pub backend_hosts: Vec<HostInfo>,
    pub shard_to_host: HashMap<Shard, HostInfo>,
}

impl TopologyConfig {
    pub fn new(coordinator_host: HostInfo, backend_hosts: Vec<HostInfo>, shard_to_host: HashMap<Shard, HostInfo>) -> Self {
        Self {
            coordinator_host,
            backend_hosts,
            shard_to_host,
        }
    }
}

/// Error type for topology operations
#[derive(Debug, Clone)]
pub enum TopologyError {
    AlreadyInitialized,
    NotInitialized,
    LockPoisoned,
}

impl std::fmt::Display for TopologyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TopologyError::AlreadyInitialized => write!(f, "Topology already initialized"),
            TopologyError::NotInitialized => write!(f, "Topology not initialized"),
            TopologyError::LockPoisoned => write!(f, "Topology lock poisoned"),
        }
    }
}

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
    pub private_ip: String,
    pub public_ip: String,
    pub internal_port: u16,
    pub http_port: u16,
}

impl NodeAddress {
    pub fn new(private_ip: String, public_ip: String, internal_port: u16, http_port: u16) -> Self {
        Self { private_ip, public_ip, internal_port, http_port }
    }

    pub fn to_address(&self) -> String {
        format!("{}:{}", self.private_ip, self.internal_port)
    }

    pub fn to_internal_address(&self) -> String {
        format!("{}:{}", self.private_ip, self.internal_port)
    }

    pub fn to_http_address(&self) -> String {
        format!("{}:{}", self.public_ip, self.http_port)
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
            
            // log!("Running periodic topology refresh...");
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterTopology {
    pub coordinator_host: HostInfo,
    pub backend_hosts: Vec<HostInfo>,
    #[serde(serialize_with = "serialize_shard_to_host", deserialize_with = "deserialize_shard_to_host")]
    pub shard_to_host: HashMap<Shard, HostInfo>
}

// Custom serialization for HashMap<Shard, HostInfo> to work with JSON
// JSON requires object keys to be strings, so we serialize as a Vec of tuples
fn serialize_shard_to_host<S>(map: &HashMap<Shard, HostInfo>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::Serialize;
    let vec: Vec<(Shard, HostInfo)> = map.iter().map(|(k, v)| (*k, v.clone())).collect();
    vec.serialize(serializer)
}

fn deserialize_shard_to_host<'de, D>(deserializer: D) -> Result<HashMap<Shard, HostInfo>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let vec: Vec<(Shard, HostInfo)> = Vec::deserialize(deserializer)?;
    Ok(vec.into_iter().collect())
}

static INSTANCE: OnceLock<RwLock<Option<Arc<ClusterTopology>>>> = OnceLock::new();

impl ClusterTopology {
    fn topology_lock() -> &'static RwLock<Option<Arc<ClusterTopology>>> {
        INSTANCE.get_or_init(|| RwLock::new(None))
    }

    /// Get the topology instance if initialized, None otherwise
    pub fn get_instance() -> Option<Arc<ClusterTopology>> {
        Self::topology_lock().read()
            .expect("ClusterTopology lock poisoned")
            .clone()
    }
    
    /// Check if ClusterTopology has been initialized
    pub fn is_initialized() -> bool {
        Self::topology_lock().read()
            .expect("ClusterTopology lock poisoned")
            .is_some()
    }
    
    /// Initialize ClusterTopology with a configuration
    /// Returns an error if already initialized
    pub fn initialize(config: TopologyConfig) -> Result<Arc<ClusterTopology>, TopologyError> {
        let topology = ClusterTopology {
            coordinator_host: config.coordinator_host,
            backend_hosts: config.backend_hosts,
            shard_to_host: config.shard_to_host,
        };
        let topology = Arc::new(topology);
        
        let mut guard = Self::topology_lock().write()
            .map_err(|_| TopologyError::LockPoisoned)?;
        
        if guard.is_some() {
            return Err(TopologyError::AlreadyInitialized);
        }
        
        *guard = Some(topology.clone());
        Ok(topology)
    }
    
    /// Initialize ClusterTopology from a ClusterTopology object (for backend use)
    /// Returns an error if already initialized
    pub fn initialize_from_topology(topology: ClusterTopology) -> Result<Arc<ClusterTopology>, TopologyError> {
        let topology = Arc::new(topology);
        
        let mut guard = Self::topology_lock().write()
            .map_err(|_| TopologyError::LockPoisoned)?;
        
        if guard.is_some() {
            return Err(TopologyError::AlreadyInitialized);
        }
        
        *guard = Some(topology.clone());
        Ok(topology)
    }
    
    /// Get default width in shards for first initialization
    pub fn default_width_in_shards() -> i32 {
        DEFAULT_WIDTH_IN_SHARDS
    }
    
    /// Get default height in shards for first initialization
    pub fn default_height_in_shards() -> i32 {
        DEFAULT_HEIGHT_IN_SHARDS
    }
    
    /// Get width in shards based on deployment mode
    /// AWS mode: 2 shards, Localhost mode: 6 shards
    pub fn width_in_shards_for_mode(deployment_mode: &str) -> i32 {
        match deployment_mode.to_lowercase().as_str() {
            "aws" => 2,
            "localhost" => DEFAULT_WIDTH_IN_SHARDS,
            _ => DEFAULT_WIDTH_IN_SHARDS,
        }
    }
    
    /// Get height in shards based on deployment mode
    /// AWS mode: 2 shards, Localhost mode: 4 shards
    pub fn height_in_shards_for_mode(deployment_mode: &str) -> i32 {
        match deployment_mode.to_lowercase().as_str() {
            "aws" => 2,
            "localhost" => DEFAULT_HEIGHT_IN_SHARDS,
            _ => DEFAULT_HEIGHT_IN_SHARDS,
        }
    }
    
    /// Get default shard width for first initialization
    pub fn default_shard_width() -> i32 {
        DEFAULT_SHARD_WIDTH
    }
    
    /// Get default shard height for first initialization
    pub fn default_shard_height() -> i32 {
        DEFAULT_SHARD_HEIGHT
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
    
    /// Calculate width in shards from the shard mapping (grid layout)
    pub fn calculate_width_in_shards(&self) -> i32 {
        if self.shard_to_host.is_empty() {
            return 0;
        }
        
        // Find the maximum x coordinate
        let max_x = self.shard_to_host.keys()
            .map(|shard| shard.x + shard.width)
            .max()
            .unwrap_or(0);
        
        // Find the minimum x coordinate
        let min_x = self.shard_to_host.keys()
            .map(|shard| shard.x)
            .min()
            .unwrap_or(0);
        
        // Get shard width from any shard
        let shard_width = self.shard_to_host.keys()
            .next()
            .map(|shard| shard.width)
            .unwrap_or(1);
        
        if shard_width == 0 {
            return 0;
        }
        
        ((max_x - min_x) / shard_width).max(1)
    }
    
    /// Calculate height in shards from the shard mapping (grid layout)
    pub fn calculate_height_in_shards(&self) -> i32 {
        if self.shard_to_host.is_empty() {
            return 0;
        }
        
        // Find the maximum y coordinate
        let max_y = self.shard_to_host.keys()
            .map(|shard| shard.y + shard.height)
            .max()
            .unwrap_or(0);
        
        // Find the minimum y coordinate
        let min_y = self.shard_to_host.keys()
            .map(|shard| shard.y)
            .min()
            .unwrap_or(0);
        
        // Get shard height from any shard
        let shard_height = self.shard_to_host.keys()
            .next()
            .map(|shard| shard.height)
            .unwrap_or(1);
        
        if shard_height == 0 {
            return 0;
        }
        
        ((max_y - min_y) / shard_height).max(1)
    }
    
    /// Get shard width from any shard in the mapping
    pub fn get_shard_width_from_mapping(&self) -> i32 {
        self.shard_to_host.keys()
            .next()
            .map(|shard| shard.width)
            .unwrap_or(0)
    }
    
    /// Get shard height from any shard in the mapping
    pub fn get_shard_height_from_mapping(&self) -> i32 {
        self.shard_to_host.keys()
            .next()
            .map(|shard| shard.height)
            .unwrap_or(0)
    }
    
    /// Get width in shards (instance method)
    pub fn width_in_shards(&self) -> i32 {
        self.calculate_width_in_shards()
    }
    
    /// Get height in shards (instance method)
    pub fn height_in_shards(&self) -> i32 {
        self.calculate_height_in_shards()
    }
    
    /// Get shard width (instance method)
    pub fn shard_width(&self) -> i32 {
        self.get_shard_width_from_mapping()
    }
    
    /// Get shard height (instance method)
    pub fn shard_height(&self) -> i32 {
        self.get_shard_height_from_mapping()
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

