use crate::cluster_topology::NodeAddress;

pub async fn discover_coordinator() -> Option<NodeAddress> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let ssm_client = aws_sdk_ssm::Client::new(&config);
    
    match ssm_client
        .get_parameter()
        .name("/colony/coordinator")
        .send()
        .await
    {
        Ok(response) => {
            if let Some(param) = response.parameter {
                if let Some(value) = param.value {
                    return parse_address(&value);
                }
            }
            None
        }
        Err(_) => None,
    }
}

pub async fn discover_backends() -> Vec<NodeAddress> {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let ssm_client = aws_sdk_ssm::Client::new(&config);
    
    let mut backends = Vec::new();
    
    match ssm_client
        .get_parameters_by_path()
        .path("/colony/backends")
        .send()
        .await
    {
        Ok(response) => {
            if let Some(params) = response.parameters {
                for param in params {
                    if let Some(value) = param.value {
                        if let Some(address) = parse_address(&value) {
                            backends.push(address);
                        }
                    }
                }
            }
        }
        Err(_) => {}
    }
    
    backends
}

pub fn parse_address(address_str: &str) -> Option<NodeAddress> {
    let parts: Vec<&str> = address_str.split(':').collect();
    if parts.len() == 2 {
        if let Ok(port) = parts[1].parse::<u16>() {
            return Some(NodeAddress::new(parts[0].to_string(), port));
        }
    }
    None
}

