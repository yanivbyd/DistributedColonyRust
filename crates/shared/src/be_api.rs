use serde::{Serialize, Deserialize};

pub const BACKEND_PORT: u16 = 8082;

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Ping(PingRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    Ping(PingResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PingRequest;

#[derive(Serialize, Deserialize, Debug)]
pub struct PingResponse; 