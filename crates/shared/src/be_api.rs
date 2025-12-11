use serde::{Serialize, Deserialize};
use std::time::{Duration};

pub const BACKEND_PORT: u16 = 8082;
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

// Re-export colony model types for backward compatibility
pub use crate::colony_model::{Color, Cell, ColonyLifeRules, Shard, ShardLayer, Traits};
pub use crate::colony_events::ColonyEvent;
pub use crate::cluster_topology::ClusterTopology;

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Ping,
    InitColony(InitColonyRequest),
    GetShardImage(GetShardImageRequest),
    GetShardLayer(GetShardLayerRequest),
    GetShardStats(GetShardStatsRequest),
    InitColonyShard(InitColonyShardRequest),
    GetColonyInfo(GetColonyInfoRequest),
    UpdatedShardContents(UpdatedShardContentsRequest),
    InitShardTopography(InitShardTopographyRequest),
    GetShardCurrentTick(GetShardCurrentTickRequest),
    ApplyEvent(ApplyEventRequest),
    StartTicking(StartTickingRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    Ping,
    InitColony(InitColonyResponse),
    GetShardImage(GetShardImageResponse),
    GetShardLayer(GetShardLayerResponse),
    GetShardStats(GetShardStatsResponse),
    InitColonyShard(InitColonyShardResponse),
    GetColonyInfo(GetColonyInfoResponse),
    UpdatedShardContents(UpdatedShardContentsResponse),
    InitShardTopography(InitShardTopographyResponse),
    GetShardCurrentTick(GetShardCurrentTickResponse),
    ApplyEvent(ApplyEventResponse),
    StartTicking(StartTickingResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyRequest {
    pub width: i32,
    pub height: i32,
    pub colony_life_rules: ColonyLifeRules,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyShardRequest {
    pub shard: Shard,
    pub colony_life_rules: ColonyLifeRules,
    pub topology: Option<ClusterTopology>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitColonyShardResponse {
    Ok,
    ShardAlreadyInitialized,
    ColonyNotInitialized,
    InvalidShardDimensions,
    Error,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitColonyResponse {
    Ok,
    ColonyAlreadyInitialized,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetShardImageRequest {
    pub shard: Shard,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum GetShardImageResponse {
    Image { image: Vec<Color> },
    ShardNotAvailable,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetShardLayerRequest {
    pub shard: Shard,
    pub layer: ShardLayer,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum GetShardLayerResponse {
    Ok { data: Vec<i32> },
    ShardNotAvailable,
}

// ===== Shard Stats API =====
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum StatMetric {
    Health,
    CreatureSize,
    CreateCanKill,
    CreateCanMove,
    Food,
    Age,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StatBucket {
    pub value: i32,
    pub occs: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShardStatResult {
    pub shard: Shard,
    pub metrics: Vec<(StatMetric, Vec<StatBucket>)>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetShardStatsRequest {
    pub shard: Shard,
    pub metrics: Vec<StatMetric>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum GetShardStatsResponse {
    Ok { stats: Vec<ShardStatResult>, tick_count: u64 },
    ColonyNotInitialized,
    ShardNotAvailable,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetColonyInfoRequest;

#[derive(Serialize, Deserialize, Debug)]
pub enum GetColonyInfoResponse {
    Ok {
        width: i32,
        height: i32,
        shards: Vec<Shard>,
        colony_life_rules: Option<ColonyLifeRules>,
        current_tick: Option<u64>,
    },
    ColonyNotInitialized,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdatedShardContentsRequest {
    pub updated_shard: Shard,
    pub top: Vec<Cell>,
    pub bottom: Vec<Cell>,
    pub left: Vec<Cell>,
    pub right: Vec<Cell>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdatedShardContentsResponse {
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitShardTopographyRequest {
    pub shard: Shard,
    pub topography_data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitShardTopographyResponse {
    Ok,
    ShardNotInitialized,
    InvalidTopographyData,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetShardCurrentTickRequest {
    pub shard: Shard,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum GetShardCurrentTickResponse {
    Ok {
        current_tick: u64,
    },
    ColonyNotInitialized,
    ShardNotAvailable,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApplyEventRequest {
    pub event: ColonyEvent,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ApplyEventResponse {
    Ok,
    ColonyNotInitialized,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StartTickingRequest {
    // Empty for now, can be extended with parameters if needed
}

#[derive(Serialize, Deserialize, Debug)]
pub enum StartTickingResponse {
    Ok,
    ColonyNotInitialized,
    TopologyNotInitialized,
    Error(String),
}
