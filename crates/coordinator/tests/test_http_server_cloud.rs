#![cfg(all(test, feature = "cloud"))]

use coordinator::http_server::HTTP_SERVER_PORT;

#[test]
fn http_server_port_constant_is_expected() {
    assert_eq!(HTTP_SERVER_PORT, 8084);
}

#[test]
fn http_server_address_format() {
    let addr = format!("127.0.0.1:{}", HTTP_SERVER_PORT);
    assert_eq!(addr, "127.0.0.1:8084");
}


