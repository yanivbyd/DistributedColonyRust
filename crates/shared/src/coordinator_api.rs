use serde::{Serialize, Deserialize};

pub const COORDINATOR_PORT: u16 = 8083;

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorRequest {
    Dummy
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorResponse {
}

