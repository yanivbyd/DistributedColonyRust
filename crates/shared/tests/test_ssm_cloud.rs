#![cfg(all(test, feature = "cloud"))]

use shared::ssm::parse_address;

#[test]
fn parse_address_valid_ipv4() {
    let addr = parse_address("192.168.1.10:8082");
    let addr = addr.expect("should parse valid address");
    assert_eq!(addr.ip, "192.168.1.10");
    assert_eq!(addr.port, 8082);
}

#[test]
fn parse_address_valid_hostname() {
    let addr = parse_address("ip-10-0-0-1.ec2.internal:9000");
    let addr = addr.expect("should parse valid hostname:port");
    assert_eq!(addr.ip, "ip-10-0-0-1.ec2.internal");
    assert_eq!(addr.port, 9000);
}

#[test]
fn parse_address_invalid_missing_port() {
    assert!(parse_address("10.0.0.1").is_none());
}

#[test]
fn parse_address_invalid_bad_port() {
    assert!(parse_address("10.0.0.1:notaport").is_none());
}


