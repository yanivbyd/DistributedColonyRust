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
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyShardRequest {
    pub shard: Shard,
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

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Cell {
    pub color: Color,
    pub tick_bit: bool,
    pub strength: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UpdatedShardContentsRequest {
    pub shard: Shard,
    pub updated_shard: Shard,
    pub top: Vec<Cell>,
    pub bottom: Vec<Cell>,
    pub left: Vec<Cell>,
    pub right: Vec<Cell>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdatedShardContentsResponse {
}
