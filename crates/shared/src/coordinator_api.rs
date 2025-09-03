use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;

pub const COORDINATOR_PORT: u16 = 8083;

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorRequest {
    GetRoutingTable,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorResponse {
    GetRoutingTableResponse { entries: Vec<RoutingEntry> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoutingEntry {
    pub shard: Shard,
    pub hostname: String,
    pub port: u16,
}

