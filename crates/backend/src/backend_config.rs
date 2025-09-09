use std::sync::OnceLock;

// Global variables for backend configuration
static BACKEND_HOSTNAME: OnceLock<String> = OnceLock::new();
static BACKEND_PORT: OnceLock<u16> = OnceLock::new();

pub fn set_backend_hostname(hostname: String) {
    BACKEND_HOSTNAME.set(hostname).expect("Failed to set hostname");
}

pub fn set_backend_port(port: u16) {
    BACKEND_PORT.set(port).expect("Failed to set port");
}

pub fn get_backend_hostname() -> &'static str {
    BACKEND_HOSTNAME.get().expect("Backend hostname not initialized")
}

pub fn get_backend_port() -> u16 {
    *BACKEND_PORT.get().expect("Backend port not initialized")
}
