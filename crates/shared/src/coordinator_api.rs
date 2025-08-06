use serde::{Serialize, Deserialize};
use crate::colony_model::{ColonyLifeInfo, Shard};

pub const COORDINATOR_PORT: u16 = 8083;

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorRequest {
    InitColony(InitColonyRequest),
    GetColonyInfo(GetColonyInfoRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum CoordinatorResponse {
    InitColony(InitColonyResponse),
    GetColonyInfo(GetColonyInfoResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyRequest {
    pub width: i32,
    pub height: i32,
    pub colony_life_info: ColonyLifeInfo,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum InitColonyResponse {
    Ok,
    ColonyAlreadyInitialized,
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