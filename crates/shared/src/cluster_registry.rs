use crate::cluster_topology::NodeAddress;
use crate::{log, log_error};
use std::sync::{Arc, OnceLock, RwLock};
use std::path::PathBuf;
use std::fs;
use serde_json;
use aws_sdk_ssm::error::ProvideErrorMetadata;

#[allow(async_fn_in_trait)]
pub trait ClusterRegistry: Send + Sync + 'static {
    async fn register_coordinator(&self, address: NodeAddress) -> Result<(), String>;
    async fn register_backend(&self, instance_id: String, address: NodeAddress) -> Result<(), String>;
    async fn discover_coordinator(&self) -> Option<NodeAddress>;
    async fn discover_backends(&self) -> Vec<NodeAddress>;
    async fn unregister_coordinator(&self) -> Result<(), String>;
    async fn unregister_backend(&self, instance_id: String) -> Result<(), String>;
}

pub struct FileClusterRegistry {
    base_path: PathBuf,
}

impl FileClusterRegistry {
    pub fn new() -> Self {
        let base_path = PathBuf::from("output/ssm");
        // Create directory structure if it doesn't exist
        if let Err(e) = fs::create_dir_all(&base_path) {
            log_error!("Failed to create ClusterRegistry directory: {}", e);
        }
        if let Err(e) = fs::create_dir_all(base_path.join("backends")) {
            log_error!("Failed to create ClusterRegistry backends directory: {}", e);
        }
        Self { base_path }
    }

    fn coordinator_path(&self) -> PathBuf {
        self.base_path.join("coordinator.json")
    }

    fn backend_path(&self, instance_id: &str) -> PathBuf {
        self.base_path.join("backends").join(format!("{}.json", instance_id))
    }
}

impl ClusterRegistry for FileClusterRegistry {
    async fn register_coordinator(&self, address: NodeAddress) -> Result<(), String> {
        let path = self.coordinator_path();
        let json = serde_json::to_string_pretty(&address)
            .map_err(|e| format!("Failed to serialize coordinator address: {}", e))?;
        
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write coordinator file: {}", e))?;
        
        log!("Registered coordinator in ClusterRegistry: {} (internal), {} (http)", 
             address.to_internal_address(), address.to_http_address());
        Ok(())
    }

    async fn register_backend(&self, instance_id: String, address: NodeAddress) -> Result<(), String> {
        let path = self.backend_path(&instance_id);
        let json = serde_json::to_string_pretty(&address)
            .map_err(|e| format!("Failed to serialize backend address: {}", e))?;
        
        fs::write(&path, json)
            .map_err(|e| format!("Failed to write backend file: {}", e))?;
        
        log!("Registered backend {} in ClusterRegistry: {} (internal), {} (http)", 
             instance_id, address.to_internal_address(), address.to_http_address());
        Ok(())
    }

    async fn discover_coordinator(&self) -> Option<NodeAddress> {
        let path = self.coordinator_path();
        match fs::read_to_string(&path) {
            Ok(content) => {
                match serde_json::from_str::<NodeAddress>(&content) {
                    Ok(address) => {
                        log!("File ClusterRegistry: discovered coordinator: {}", address.to_internal_address());
                        Some(address)
                    }
                    Err(e) => {
                        log_error!("Failed to parse coordinator file: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log_error!("Failed to read coordinator file: {}", e);
                }
                None
            }
        }
    }

    async fn discover_backends(&self) -> Vec<NodeAddress> {
        let backends_dir = self.base_path.join("backends");
        let mut backends = Vec::new();
        
        match fs::read_dir(&backends_dir) {
            Ok(entries) => {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.extension().and_then(|s| s.to_str()) == Some("json") {
                            match fs::read_to_string(&path) {
                                Ok(content) => {
                                    match serde_json::from_str::<NodeAddress>(&content) {
                                        Ok(address) => {
                                            backends.push(address);
                                        }
                                        Err(e) => {
                                            log_error!("Failed to parse backend file {:?}: {}", path, e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    log_error!("Failed to read backend file {:?}: {}", path, e);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log_error!("Failed to read backends directory: {}", e);
                }
            }
        }
        
        log!("File ClusterRegistry: discovered {} backend entries", backends.len());
        backends
    }

    async fn unregister_coordinator(&self) -> Result<(), String> {
        let path = self.coordinator_path();
        match fs::remove_file(&path) {
            Ok(()) => {
                log!("Unregistered coordinator from ClusterRegistry");
                Ok(())
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    // File doesn't exist, consider it already unregistered
                    Ok(())
                } else {
                    let err = format!("Failed to remove coordinator file: {}", e);
                    log_error!("{}", err);
                    Err(err)
                }
            }
        }
    }

    async fn unregister_backend(&self, instance_id: String) -> Result<(), String> {
        let path = self.backend_path(&instance_id);
        match fs::remove_file(&path) {
            Ok(()) => {
                log!("Unregistered backend {} from ClusterRegistry", instance_id);
                Ok(())
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::NotFound {
                    // File doesn't exist, consider it already unregistered
                    Ok(())
                } else {
                    let err = format!("Failed to remove backend file: {}", e);
                    log_error!("{}", err);
                    Err(err)
                }
            }
        }
    }
}

pub struct SsmClusterRegistry {
    // AWS SSM client will be created on-demand
}

impl SsmClusterRegistry {
    pub fn new() -> Self {
        Self {}
    }
}

impl ClusterRegistry for SsmClusterRegistry {
    async fn register_coordinator(&self, address: NodeAddress) -> Result<(), String> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let ssm_client = aws_sdk_ssm::Client::new(&config);
        
        let json_value = serde_json::to_string(&address)
            .map_err(|e| format!("Failed to serialize coordinator address: {}", e))?;
        
        match ssm_client
            .put_parameter()
            .name("/colony/coordinator")
            .value(&json_value)
            .overwrite(true)
            .send()
            .await
        {
            Ok(_) => {
                log!("Registered coordinator in SSM ClusterRegistry: {} (internal), {} (http)", 
                     address.to_internal_address(), address.to_http_address());
                Ok(())
            }
            Err(err) => {
                let error_msg = format!("Failed to register coordinator in SSM: {}", err);
                log_error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }

    async fn register_backend(&self, instance_id: String, address: NodeAddress) -> Result<(), String> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let ssm_client = aws_sdk_ssm::Client::new(&config);
        
        let json_value = serde_json::to_string(&address)
            .map_err(|e| format!("Failed to serialize backend address: {}", e))?;
        
        let param_name = format!("/colony/backends/{}", instance_id);
        match ssm_client
            .put_parameter()
            .name(&param_name)
            .value(&json_value)
            .overwrite(true)
            .send()
            .await
        {
            Ok(_) => {
                log!("Registered backend {} in SSM ClusterRegistry: {} (internal), {} (http)", 
                     instance_id, address.to_internal_address(), address.to_http_address());
                Ok(())
            }
            Err(err) => {
                let error_msg = format!("Failed to register backend {} in SSM: {}", instance_id, err);
                log_error!("{}", error_msg);
                Err(error_msg)
            }
        }
    }

    async fn discover_coordinator(&self) -> Option<NodeAddress> {
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
                        match parse_address_json(&value) {
                            Some(address) => {
                                log!("SSM ClusterRegistry: coordinator entry = {}", address.to_internal_address());
                                return Some(address);
                            }
                            None => {
                                log_error!("SSM: coordinator entry has invalid format");
                            }
                        }
                    }
                }
                log!("SSM: coordinator entry missing or invalid");
                None
            }
            Err(err) => {
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

    async fn discover_backends(&self) -> Vec<NodeAddress> {
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
                            if let Some(address) = parse_address_json(&value) {
                                backends.push(address);
                            }
                        }
                    }
                }
                log!("SSM ClusterRegistry: discovered {} backend entries", backends.len());
            }
            Err(err) => {
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

    async fn unregister_coordinator(&self) -> Result<(), String> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let ssm_client = aws_sdk_ssm::Client::new(&config);
        
        match ssm_client
            .delete_parameter()
            .name("/colony/coordinator")
            .send()
            .await
        {
            Ok(_) => {
                log!("Unregistered coordinator from SSM ClusterRegistry");
                Ok(())
            }
            Err(err) => {
                let error_code = err.code();
                if let Some(code) = error_code {
                    if code == "ParameterNotFound" {
                        // Parameter doesn't exist, consider it already unregistered
                        Ok(())
                    } else {
                        let error_msg = format!("Failed to unregister coordinator from SSM: {}", err);
                        log_error!("{}", error_msg);
                        Err(error_msg)
                    }
                } else {
                    let error_msg = format!("Failed to unregister coordinator from SSM: {}", err);
                    log_error!("{}", error_msg);
                    Err(error_msg)
                }
            }
        }
    }

    async fn unregister_backend(&self, instance_id: String) -> Result<(), String> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let ssm_client = aws_sdk_ssm::Client::new(&config);
        
        let param_name = format!("/colony/backends/{}", instance_id);
        match ssm_client
            .delete_parameter()
            .name(&param_name)
            .send()
            .await
        {
            Ok(_) => {
                log!("Unregistered backend {} from SSM ClusterRegistry", instance_id);
                Ok(())
            }
            Err(err) => {
                let error_code = err.code();
                if let Some(code) = error_code {
                    if code == "ParameterNotFound" {
                        // Parameter doesn't exist, consider it already unregistered
                        Ok(())
                    } else {
                        let error_msg = format!("Failed to unregister backend {} from SSM: {}", instance_id, err);
                        log_error!("{}", error_msg);
                        Err(error_msg)
                    }
                } else {
                    let error_msg = format!("Failed to unregister backend {} from SSM: {}", instance_id, err);
                    log_error!("{}", error_msg);
                    Err(error_msg)
                }
            }
        }
    }
}

fn parse_address_json(json_str: &str) -> Option<NodeAddress> {
    match serde_json::from_str::<NodeAddress>(json_str) {
        Ok(address) => Some(address),
        Err(e) => {
            log_error!("Failed to parse address JSON: {} - {}", json_str, e);
            None
        }
    }
}

pub enum ClusterRegistryImpl {
    File(FileClusterRegistry),
    Ssm(SsmClusterRegistry),
}

impl ClusterRegistry for ClusterRegistryImpl {
    async fn register_coordinator(&self, address: NodeAddress) -> Result<(), String> {
        match self {
            ClusterRegistryImpl::File(reg) => reg.register_coordinator(address).await,
            ClusterRegistryImpl::Ssm(reg) => reg.register_coordinator(address).await,
        }
    }

    async fn register_backend(&self, instance_id: String, address: NodeAddress) -> Result<(), String> {
        match self {
            ClusterRegistryImpl::File(reg) => reg.register_backend(instance_id, address).await,
            ClusterRegistryImpl::Ssm(reg) => reg.register_backend(instance_id, address).await,
        }
    }

    async fn discover_coordinator(&self) -> Option<NodeAddress> {
        match self {
            ClusterRegistryImpl::File(reg) => reg.discover_coordinator().await,
            ClusterRegistryImpl::Ssm(reg) => reg.discover_coordinator().await,
        }
    }

    async fn discover_backends(&self) -> Vec<NodeAddress> {
        match self {
            ClusterRegistryImpl::File(reg) => reg.discover_backends().await,
            ClusterRegistryImpl::Ssm(reg) => reg.discover_backends().await,
        }
    }

    async fn unregister_coordinator(&self) -> Result<(), String> {
        match self {
            ClusterRegistryImpl::File(reg) => reg.unregister_coordinator().await,
            ClusterRegistryImpl::Ssm(reg) => reg.unregister_coordinator().await,
        }
    }

    async fn unregister_backend(&self, instance_id: String) -> Result<(), String> {
        match self {
            ClusterRegistryImpl::File(reg) => reg.unregister_backend(instance_id).await,
            ClusterRegistryImpl::Ssm(reg) => reg.unregister_backend(instance_id).await,
        }
    }
}

static REGISTRY_INSTANCE: OnceLock<RwLock<Option<Arc<ClusterRegistryImpl>>>> = OnceLock::new();

pub fn create_cluster_registry(deployment_mode: &str) -> Arc<ClusterRegistryImpl> {
    let registry: Arc<ClusterRegistryImpl> = match deployment_mode.to_lowercase().as_str() {
        "aws" => Arc::new(ClusterRegistryImpl::Ssm(SsmClusterRegistry::new())),
        _ => Arc::new(ClusterRegistryImpl::File(FileClusterRegistry::new())),
    };
    
    let cell = REGISTRY_INSTANCE.get_or_init(|| RwLock::new(None));
    if let Ok(mut guard) = cell.write() {
        *guard = Some(registry.clone());
    }
    
    registry
}

pub fn get_instance() -> Option<Arc<ClusterRegistryImpl>> {
    let cell = REGISTRY_INSTANCE.get_or_init(|| RwLock::new(None));
    cell.read().ok().and_then(|g| g.clone())
}
