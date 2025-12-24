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
    pub original_color: Color,
    pub health: u16,
    pub age: u16,

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
    pub random_death_chance: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shard {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Shard {
    /// Converts `Shard` struct to shard_id string format: `{x}_{y}_{width}_{height}`
    pub fn to_id(&self) -> String {
        format!("{}_{}_{}_{}", self.x, self.y, self.width, self.height)
    }
    
    /// Parses shard_id string to `Shard` struct
    /// Returns `Ok(Shard)` if format is valid, `Err(String)` with error message if invalid
    pub fn from_id(id: &str) -> Result<Self, String> {
        let parts: Vec<&str> = id.split('_').collect();
        if parts.len() != 4 {
            return Err(format!("Invalid shard_id format: expected 4 parts separated by '_', got {}", parts.len()));
        }
        let x = parts[0].parse::<i32>()
            .map_err(|e| format!("Invalid x coordinate '{}': {}", parts[0], e))?;
        let y = parts[1].parse::<i32>()
            .map_err(|e| format!("Invalid y coordinate '{}': {}", parts[1], e))?;
        let width = parts[2].parse::<i32>()
            .map_err(|e| format!("Invalid width '{}': {}", parts[2], e))?;
        let height = parts[3].parse::<i32>()
            .map_err(|e| format!("Invalid height '{}': {}", parts[3], e))?;
        Ok(Shard { x, y, width, height })
    }
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
    Age,
} 