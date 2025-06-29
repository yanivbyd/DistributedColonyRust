use serde::{Serialize, Deserialize};
use std::time::Duration;

pub const BACKEND_PORT: u16 = 8082;
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Ping,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    Ping,
} 