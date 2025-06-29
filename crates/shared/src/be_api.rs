use serde::{Serialize, Deserialize};
use std::time::Duration;

pub const BACKEND_PORT: u16 = 8082;
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Ping,
    InitColony(InitColonyRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    Ping,
    InitColony,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyRequest {
    pub width: i32,
    pub height: i32,
}
