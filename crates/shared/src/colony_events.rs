use serde::{Serialize, Deserialize};
use crate::colony_model::{Color, Traits, ColonyLifeRules};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Ellipse {
    pub x: i32,
    pub y: i32,
    pub radius_x: i32,
    pub radius_y: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Region {
    Ellipse(Ellipse),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CreateCreatureParams {
    pub color: Color,
    pub traits: Traits,
    pub starting_health: u16,
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ColonyEvent {
    CreateCreature(Region, CreateCreatureParams),
    ChangeExtraFoodPerTick(i8),
    Extinction(),
    NewTopography(),
    ChangeColonyRules(ColonyRuleChange)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ColonyRuleChange {
    pub new_rules: ColonyLifeRules,
    pub description: String,
}
