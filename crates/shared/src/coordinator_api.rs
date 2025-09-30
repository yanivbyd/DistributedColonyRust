use serde::{Serialize, Deserialize};
use crate::colony_model::Shard;
use crate::be_api::{StatMetric, StatBucket};

pub const COORDINATOR_PORT: u16 = 8083;

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorRequest {
    GetRoutingTable,
    GetColonyEvents { limit: usize },
    GetColonyStats { metrics: Vec<StatMetric> },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorResponse {
    GetRoutingTableResponse { entries: Vec<RoutingEntry> },
    GetColonyEventsResponse { events: Vec<ColonyEventDescription> },
    GetColonyStatsResponse { metrics: Vec<ColonyMetricStats>, tick_count: u64 },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColonyMetricStats {
    pub metric: StatMetric,
    pub avg: f64,
    pub buckets: Vec<StatBucket>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoutingEntry {
    pub shard: Shard,
    pub hostname: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColonyEventDescription {
    pub tick: u64,
    pub event_type: String,
    pub description: String,
}

