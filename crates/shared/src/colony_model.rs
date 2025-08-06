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

#[derive(Debug, Clone)]
pub struct ShardTopographyInfo {
    pub default_value: u8,
    pub top_border: Vec<u8>,     
    pub bottom_border: Vec<u8>,  
    pub left_border: Vec<u8>,    
    pub right_border: Vec<u8>,   
    pub points: Vec<(u16, u16, u8)>, 
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum ShardLayer {
    CreatureSize,
    ExtraFood,
} 