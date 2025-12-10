use crate::cluster_topology::NodeAddress;
use crate::cluster_registry::{ClusterRegistry, get_instance};
use crate::{log_error, log};
use std::sync::{Arc, RwLock, OnceLock};
use aws_sdk_ssm::error::ProvideErrorMetadata;

pub trait SsmProvider: Send + Sync + 'static {
    fn discover_coordinator(&self) -> Option<NodeAddress>;
    fn discover_backends(&self) -> Vec<NodeAddress>;
}

static MOCK_PROVIDER: OnceLock<RwLock<Option<Arc<dyn SsmProvider>>>> = OnceLock::new();

fn get_mock_provider() -> Option<Arc<dyn SsmProvider>> {
    let cell = MOCK_PROVIDER.get_or_init(|| RwLock::new(None));
    cell.read().ok().and_then(|g| g.clone())
}

pub fn set_mock_provider(provider: Option<Arc<dyn SsmProvider>>) {
    let cell = MOCK_PROVIDER.get_or_init(|| RwLock::new(None));
    if let Ok(mut guard) = cell.write() {
        *guard = provider;
    }
}

pub async fn discover_coordinator() -> Option<NodeAddress> {
    if let Some(provider) = get_mock_provider() {
        return provider.discover_coordinator();
    }

    // Try to use ClusterRegistry if available
    if let Some(registry) = get_instance() {
        return registry.discover_coordinator().await;
    }

    // Fallback to direct SSM access (for backward compatibility)
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
                    if let Some(address) = parse_address(&value) {
                        log!("SSM: coordinator entry = {}", address.to_address());
                        return Some(address);
                    }
                }
            }
            log!("SSM: coordinator entry missing or invalid");
            None
        }
        Err(err) => {
            // Check if it's a parameter not found error (expected initially)
            let error_code = err.code();
            if let Some(code) = error_code {
                if code == "ParameterNotFound" {
                    log!("SSM: coordinator parameter not found yet (this is normal during startup)");
                } else {
                    log_error!("Failed to read coordinator from SSM: {} (code: {:?}, message: {:?})", 
                        err, code, err.message());
                }
            } else {
                log_error!("Failed to read coordinator from SSM: {} (full error: {:?})", err, err);
            }
            None
        }
    }
}

pub async fn discover_backends() -> Vec<NodeAddress> {
    if let Some(provider) = get_mock_provider() {
        return provider.discover_backends();
    }

    // Try to use ClusterRegistry if available
    if let Some(registry) = get_instance() {
        return registry.discover_backends().await;
    }

    // Fallback to direct SSM access (for backward compatibility)
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
            log!("SSM: discovered {} backend entries", backends.len());
        }
        Err(err) => {
            // Check if it's a parameter not found error (expected initially)
            let error_code = err.code();
            if let Some(code) = error_code {
                if code == "ParameterNotFound" || code == "InvalidKeyId" {
                    log!("SSM: backend parameters not found yet (this is normal during startup)");
                } else {
                    log_error!("Failed to read backends from SSM: {} (code: {:?}, message: {:?})", 
                        err, code, err.message());
                }
            } else {
                log_error!("Failed to read backends from SSM: {} (full error: {:?})", err, err);
            }
        }
    }
    
    backends
}

pub fn parse_address(address_str: &str) -> Option<NodeAddress> {
    // First try to parse as JSON (new format with internal_port and http_port)
    if let Ok(address) = serde_json::from_str::<NodeAddress>(address_str) {
        return Some(address);
    }
    
    // Fallback to old format (ip:port) for backward compatibility
    let parts: Vec<&str> = address_str.split(':').collect();
    if parts.len() == 2 {
        if let Ok(port) = parts[1].parse::<u16>() {
            // For backward compatibility, use the same port for both internal and http
            return Some(NodeAddress::new(parts[0].to_string(), port, port));
        }
    }
    None
}

