#![cfg(all(test, feature = "cloud"))]

use shared::ssm::parse_address;

#[test]
fn parse_address_valid_ipv4() {
    let addr = parse_address("192.168.1.10:8082");
    let addr = addr.expect("should parse valid address");
    assert_eq!(addr.private_ip, "192.168.1.10");
    assert_eq!(addr.public_ip, "192.168.1.10"); // Backward compatibility: same IP for both
    assert_eq!(addr.internal_port, 8082);
    assert_eq!(addr.http_port, 8082); // Backward compatibility: same port for both
}

#[test]
fn parse_address_valid_hostname() {
    let addr = parse_address("ip-10-0-0-1.ec2.internal:9000");
    let addr = addr.expect("should parse valid hostname:port");
    assert_eq!(addr.private_ip, "ip-10-0-0-1.ec2.internal");
    assert_eq!(addr.public_ip, "ip-10-0-0-1.ec2.internal"); // Backward compatibility: same IP for both
    assert_eq!(addr.internal_port, 9000);
    assert_eq!(addr.http_port, 9000); // Backward compatibility: same port for both
}

#[test]
fn parse_address_json_format() {
    let addr = parse_address(r#"{"private_ip":"192.168.1.10","public_ip":"3.252.213.4","internal_port":8082,"http_port":8084}"#);
    let addr = addr.expect("should parse valid JSON address");
    assert_eq!(addr.private_ip, "192.168.1.10");
    assert_eq!(addr.public_ip, "3.252.213.4");
    assert_eq!(addr.internal_port, 8082);
    assert_eq!(addr.http_port, 8084);
}

#[test]
fn parse_address_invalid_missing_port() {
    assert!(parse_address("10.0.0.1").is_none());
}

#[test]
fn parse_address_invalid_bad_port() {
    assert!(parse_address("10.0.0.1:notaport").is_none());
}


