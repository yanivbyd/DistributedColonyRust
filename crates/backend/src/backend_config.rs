use std::sync::OnceLock;

// Global variables for backend configuration
static BACKEND_HOSTNAME: OnceLock<String> = OnceLock::new();
static BACKEND_PORT: OnceLock<u16> = OnceLock::new();
static DEPLOYMENT_MODE: OnceLock<String> = OnceLock::new();

pub fn set_backend_hostname(hostname: String) {
    BACKEND_HOSTNAME.set(hostname).expect("Failed to set hostname");
}

pub fn set_backend_port(port: u16) {
    BACKEND_PORT.set(port).expect("Failed to set port");
}

pub fn set_deployment_mode(mode: String) {
    DEPLOYMENT_MODE.set(mode).expect("Failed to set deployment mode");
}

pub fn get_backend_hostname() -> &'static str {
    BACKEND_HOSTNAME.get().expect("Backend hostname not initialized")
}

pub fn get_backend_port() -> u16 {
    *BACKEND_PORT.get().expect("Backend port not initialized")
}

pub fn is_aws_deployment() -> bool {
    DEPLOYMENT_MODE.get()
        .map(|mode| mode.as_str() == "aws")
        .unwrap_or(false)
}
