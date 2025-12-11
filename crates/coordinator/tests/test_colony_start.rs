#![cfg(all(test, feature = "cloud"))]

use shared::cluster_topology::{NodeAddress, HostInfo};

/// Test helper function that implements the core filtering logic without the status check.
/// This allows us to test the IP+port comparison logic in isolation.
fn filter_backends_by_address_only(
    backend_addresses: Vec<NodeAddress>,
    coordinator_address: &NodeAddress,
) -> Vec<HostInfo> {
    let coordinator_internal_port = coordinator_address.internal_port;
    let mut available_backends = Vec::new();
    
    for backend_address in backend_addresses {
        // Skip if this backend matches the coordinator's address (same IP and port)
        // In localhost mode, IPs will match, so we check the port
        if backend_address.ip == coordinator_address.ip && 
           backend_address.internal_port == coordinator_internal_port {
            continue;
        }
        
        // For this test, we skip the status check and just add all non-coordinator backends
        available_backends.push(HostInfo::new(
            backend_address.ip,
            backend_address.internal_port,
        ));
    }
    
    available_backends
}

#[test]
fn test_filter_backends_excluding_coordinator_localhost_mode() {
    // Simulate localhost mode: coordinator and all backends share the same IP (127.0.0.1)
    // This is the critical case that was failing before the fix
    let coordinator = NodeAddress::new("127.0.0.1".to_string(), 8082, 8083);
    
    // Create backend addresses with same IP but different ports
    let backend_addresses = vec![
        NodeAddress::new("127.0.0.1".to_string(), 8084, 8085), // Backend 1
        NodeAddress::new("127.0.0.1".to_string(), 8086, 8087), // Backend 2
        NodeAddress::new("127.0.0.1".to_string(), 8088, 8089), // Backend 3
        NodeAddress::new("127.0.0.1".to_string(), 8090, 8091), // Backend 4
        NodeAddress::new("127.0.0.1".to_string(), 8082, 8083), // Coordinator (should be filtered out)
    ];
    
    let filtered = filter_backends_by_address_only(backend_addresses, &coordinator);
    
    // Verify that all backends with different ports are included
    assert_eq!(filtered.len(), 4, "Should include 4 backends (coordinator excluded)");
    
    // Verify coordinator port is not in results
    let coordinator_ports: Vec<u16> = filtered.iter().map(|b| b.port).collect();
    assert!(
        !coordinator_ports.contains(&coordinator.internal_port),
        "Coordinator port {} should be filtered out",
        coordinator.internal_port
    );
    
    // Verify all backend ports are present
    let expected_ports = vec![8084, 8086, 8088, 8090];
    for expected_port in expected_ports {
        assert!(
            coordinator_ports.contains(&expected_port),
            "Backend port {} should be included",
            expected_port
        );
    }
}

#[test]
fn test_filter_backends_excluding_coordinator_different_ips() {
    // Simulate AWS mode: coordinator and backends have different IPs
    let coordinator = NodeAddress::new("10.0.1.1".to_string(), 8082, 8083);
    
    let backend_addresses = vec![
        NodeAddress::new("10.0.1.2".to_string(), 8084, 8085), // Backend 1
        NodeAddress::new("10.0.1.3".to_string(), 8086, 8087), // Backend 2
        NodeAddress::new("10.0.1.1".to_string(), 8082, 8083), // Coordinator (same IP+port, should be filtered)
        NodeAddress::new("10.0.1.4".to_string(), 8088, 8089), // Backend 3
    ];
    
    let filtered = filter_backends_by_address_only(backend_addresses, &coordinator);
    
    // Verify coordinator is not in results
    assert_eq!(filtered.len(), 3, "Should include 3 backends (coordinator excluded)");
    
    let coordinator_ports: Vec<u16> = filtered.iter().map(|b| b.port).collect();
    assert!(
        !coordinator_ports.contains(&coordinator.internal_port),
        "Coordinator port {} should not appear in filtered backends",
        coordinator.internal_port
    );
}

#[test]
fn test_filter_backends_excluding_coordinator_same_ip_different_port() {
    // Test the critical localhost case: same IP, different ports
    // This verifies that comparing only IP would be wrong
    let coordinator = NodeAddress::new("127.0.0.1".to_string(), 8082, 8083);
    
    let backend_addresses = vec![
        NodeAddress::new("127.0.0.1".to_string(), 8084, 8085), // Should NOT be filtered (different port)
        NodeAddress::new("127.0.0.1".to_string(), 8086, 8087), // Should NOT be filtered (different port)
        NodeAddress::new("127.0.0.1".to_string(), 8082, 8083), // SHOULD be filtered (same IP AND port)
    ];
    
    let filtered = filter_backends_by_address_only(backend_addresses, &coordinator);
    
    // Key assertion: backends with same IP but different ports should NOT be filtered
    assert_eq!(filtered.len(), 2, "Should include 2 backends with different ports");
    
    // Verify coordinator is filtered out
    let coordinator_ports: Vec<u16> = filtered.iter().map(|b| b.port).collect();
    assert!(
        !coordinator_ports.contains(&coordinator.internal_port),
        "Coordinator port {} should be filtered out",
        coordinator.internal_port
    );
    
    // Verify the different-port backends are included
    assert!(coordinator_ports.contains(&8084), "Backend port 8084 should be included");
    assert!(coordinator_ports.contains(&8086), "Backend port 8086 should be included");
}
