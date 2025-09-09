use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

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
    
    fn new_fixed_topology() -> Self {
        let coordinator_host = HostInfo::new("127.0.0.1".to_string(), 8083);
        let backend_host = HostInfo::new("127.0.0.1".to_string(), 8082);
        let shards = Self::create_fixed_shards();
        
        let mut shard_to_host = HashMap::new();
        
        for shard in &shards {
            shard_to_host.insert(*shard, backend_host.clone());
        }
        
        Self {
            coordinator_host,
            backend_hosts: vec![backend_host],
            shard_to_host
        }
    }
    
    fn create_fixed_shards() -> Vec<Shard> {
        let mut shards = Vec::new();
        let shard_width = 250;
        let shard_height = 250;
        
        // Create 5x3 grid of shards (5 columns, 3 rows)
        for y in 0..3 {
            for x in 0..5 {
                let shard = Shard {
                    x: (x * shard_width) as i32,
                    y: (y * shard_height) as i32,
                    width: shard_width,
                    height: shard_height,
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

