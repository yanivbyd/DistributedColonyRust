use serde::{Serialize, Deserialize};
use std::time::Duration;

pub const BACKEND_PORT: u16 = 8082;
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Color {
    pub fn equals(&self, other: &Color) -> bool {
        self.red == other.red && self.green == other.green && self.blue == other.blue
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Cell {
    pub tick_bit: bool,

    // Cell itself
    pub food: u8,
    pub extra_food_per_tick: u8,

    // Creature 
    pub color: Color,
    pub health: u8,

    pub traits: Traits,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Traits {
    pub size: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ColonyLifeInfo {
    pub health_cost_per_size_unit: u8,
    pub eat_capacity_per_size_unit: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shard {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Ping,
    InitColony(InitColonyRequest),
    GetShardImage(GetShardImageRequest),
    InitColonyShard(InitColonyShardRequest),
    GetColonyInfo(GetColonyInfoRequest),
    UpdatedShardContents(UpdatedShardContentsRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    Ping,
    InitColony(InitColonyResponse),
    GetShardImage(GetShardImageResponse),
    InitColonyShard(InitColonyShardResponse),
    GetColonyInfo(GetColonyInfoResponse),
    UpdatedShardContents(UpdatedShardContentsResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyRequest {
    pub width: i32,
    pub height: i32,
    pub colony_life_info: ColonyLifeInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyShardRequest {
    pub shard: Shard,
    pub colony_life_info: ColonyLifeInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitColonyShardResponse {
    Ok,
    ShardAlreadyInitialized,
    ColonyNotInitialized,
    InvalidShardDimensions
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
pub struct GetColonyInfoRequest;

#[derive(Serialize, Deserialize, Debug)]
pub enum GetColonyInfoResponse {
    Ok {
        width: i32,
        height: i32,
        shards: Vec<Shard>,
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
