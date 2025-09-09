use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;
use std::collections::{HashMap, HashSet};

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
    pub fn new_fixed_topology() -> Self {
        let coordinator_host = HostInfo::new("127.0.0.1".to_string(), 8083);
        let backend_host = HostInfo::new("127.0.0.1".to_string(), 8082);
        let shards = Self::create_fixed_shards();
        
        let mut shard_to_host = HashMap::new();
        
        // All shards are hosted by the single backend
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
}

