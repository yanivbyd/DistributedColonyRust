use serde::{Serialize, Deserialize};

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
    pub food: u16,
    pub extra_food_per_tick: u8,

    // Creature 
    pub color: Color,
    pub health: u16,

    pub traits: Traits,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct Traits {
    pub size: u8,
    pub can_kill: bool,
    pub can_move: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ColonyLifeRules {
    pub health_cost_per_size_unit: u32,
    pub eat_capacity_per_size_unit: u32,
    pub health_cost_if_can_kill: u32,
    pub health_cost_if_can_move: u32,
    pub mutation_chance: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shard {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ShardLayer {
    CreatureSize,
    ExtraFood,
    CanKill,
    CanMove,
    CostPerTurn,
    Food,
    Health,
} 