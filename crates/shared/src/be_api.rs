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
    /// Returns true if the color is white (all RGB components are 255)
    pub fn is_white(&self) -> bool {
        self.red == 255 && self.green == 255 && self.blue == 255
    }
    /// Returns true if the color is different from another color
    pub fn is_different(&self, other: &Color) -> bool {
        self.red != other.red || self.green != other.green || self.blue != other.blue
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    Ping,
    InitColony(InitColonyRequest),
    GetSubImage(GetSubImageRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    Ping,
    InitColony,
    GetSubImage(GetSubImageResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InitColonyRequest {
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetSubImageRequest {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetSubImageResponse {
    pub colors: Vec<Color>,
}
