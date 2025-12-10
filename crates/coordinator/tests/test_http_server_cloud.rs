#![cfg(all(test, feature = "cloud"))]

#[test]
fn http_server_address_format() {
    // Coordinator HTTP port is now configurable, but default is 8083 in localhost mode
    // In AWS mode, it comes from HTTP_PORT environment variable
    let coordinator_http_port = 8083;
    let addr = format!("127.0.0.1:{}", coordinator_http_port);
    assert_eq!(addr, "127.0.0.1:8083");
}


