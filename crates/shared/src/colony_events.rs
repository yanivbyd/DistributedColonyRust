use serde::{Serialize, Deserialize};
use crate::colony_model::{Color, Traits};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Circle {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Ellipse {
    pub x: i32,
    pub y: i32,
    pub radius_x: i32,
    pub radius_y: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Region {
    Circle(Circle),
    Ellipse(Ellipse),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateCreatureParams {
    pub color: Color,
    pub traits: Traits,
    pub starting_health: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RandomTraitParams {
    pub traits: Traits,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ColonyEvent {
    LocalDeath(Region),
    RandomTrait(Region, RandomTraitParams),
    CreateCreature(Region, CreateCreatureParams),
    ChangeExtraFoodPerTick(i8),
    Extinction(),
    NewTopography()
}
