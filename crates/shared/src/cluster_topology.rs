use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// Configuration constants
const COORDINATOR_PORT: u16 = 8083;
const BACKEND_PORTS: &[u16] = &[8082, 8084, 8085, 8086];
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
    pub port: u16,
}

impl NodeAddress {
    pub fn new(ip: String, port: u16) -> Self {
        Self { ip, port }
    }

    pub fn to_address(&self) -> String {
        format!("{}:{}", self.ip, self.port)
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
    pub coordinator_info: NodeInfo,
    pub backend_info: Vec<NodeInfo>,
}

impl DiscoveredTopology {
    pub fn new(
        self_type: NodeType,
        self_address: NodeAddress,
        coordinator_info: NodeInfo,
        backend_info: Vec<NodeInfo>,
    ) -> Self {
        Self {
            self_type,
            self_address,
            coordinator_info,
            backend_info,
        }
    }
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

impl ClusterTopology {
    pub fn get_instance() -> &'static ClusterTopology {
        static INSTANCE: OnceLock<ClusterTopology> = OnceLock::new();
        INSTANCE.get_or_init(|| Self::new_fixed_topology())
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

