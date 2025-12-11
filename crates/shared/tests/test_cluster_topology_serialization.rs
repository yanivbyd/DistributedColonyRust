#[cfg(test)]
mod tests {
    use shared::cluster_topology::{ClusterTopology, HostInfo};
    use shared::colony_model::Shard;
    use std::collections::HashMap;
    use serde_json;

    #[test]
    fn test_cluster_topology_json_serialization_roundtrip() {
        // Create a ClusterTopology with shard_to_host mapping
        let coordinator_host = HostInfo::new("127.0.0.1".to_string(), 8082);
        let backend_hosts = vec![
            HostInfo::new("127.0.0.1".to_string(), 8084),
            HostInfo::new("127.0.0.1".to_string(), 8086),
        ];
        
        let mut shard_to_host = HashMap::new();
        let shard1 = Shard { x: 0, y: 0, width: 250, height: 250 };
        let shard2 = Shard { x: 250, y: 0, width: 250, height: 250 };
        shard_to_host.insert(shard1, HostInfo::new("127.0.0.1".to_string(), 8084));
        shard_to_host.insert(shard2, HostInfo::new("127.0.0.1".to_string(), 8086));
        
        let topology = ClusterTopology {
            coordinator_host: coordinator_host.clone(),
            backend_hosts: backend_hosts.clone(),
            shard_to_host: shard_to_host.clone(),
        };
        
        // This test expects serialization to succeed (will fail until we fix the HashMap serialization)
        let json = serde_json::to_string(&topology)
            .expect("Failed to serialize ClusterTopology to JSON");
        
        // Verify we can deserialize it back
        let deserialized: ClusterTopology = serde_json::from_str(&json)
            .expect("Failed to deserialize ClusterTopology from JSON");
        
        // Verify the roundtrip preserved all data
        assert_eq!(deserialized.coordinator_host, coordinator_host);
        assert_eq!(deserialized.backend_hosts, backend_hosts);
        assert_eq!(deserialized.shard_to_host.len(), shard_to_host.len());
        assert_eq!(
            deserialized.shard_to_host.get(&shard1),
            shard_to_host.get(&shard1)
        );
        assert_eq!(
            deserialized.shard_to_host.get(&shard2),
            shard_to_host.get(&shard2)
        );
    }
}
