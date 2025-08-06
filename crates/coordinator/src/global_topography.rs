// Global topography module for the coordinator
// This module will handle global topography-related functionality

use shared::be_api::{Shard, BackendRequest, BackendResponse, InitShardTopographyRequest, InitShardTopographyResponse, BACKEND_PORT};
use shared::{log, log_error};
use shared::colony_model::ShardTopographyInfo;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bincode;

pub struct GlobalTopographyInfo {
    pub total_width: usize,
    pub total_height: usize,
    pub shard_width: usize,
    pub shard_height: usize,
    pub default_value: u8,
    pub points_per_subgrid: u8,
    pub points_min_max_value: (u8, u8),
}

pub struct GlobalTopography {
    info: GlobalTopographyInfo,
}

impl GlobalTopography {
    pub fn new(info: GlobalTopographyInfo) -> Self {
        Self { info }
    }

    async fn send_topography_to_local_shard(&self, shard: Shard, info: ShardTopographyInfo) {
        let request = BackendRequest::InitShardTopography(InitShardTopographyRequest {
            shard,
            topography_info: info,
        });

        if let Ok(mut stream) = TcpStream::connect(format!("127.0.0.1:{}", BACKEND_PORT)).await {
            if let Err(e) = Self::send_message(&mut stream, &request).await {
                log_error!("[COORD] Failed to send topography to shard ({},{},{},{}): {}", 
                    shard.x, shard.y, shard.width, shard.height, e);
                return;
            }

            if let Some(response) = Self::receive_message::<BackendResponse>(&mut stream).await {
                match response {
                    BackendResponse::InitShardTopography(InitShardTopographyResponse::Ok) => {
                        log!("[COORD] Topography sent to shard ({},{},{},{})", 
                            shard.x, shard.y, shard.width, shard.height);
                    },
                    BackendResponse::InitShardTopography(InitShardTopographyResponse::ShardNotInitialized) => {
                        log_error!("[COORD] Shard not initialized for topography: ({},{},{},{})", 
                            shard.x, shard.y, shard.width, shard.height);
                    },
                    BackendResponse::InitShardTopography(InitShardTopographyResponse::InvalidTopographyData) => {
                        log_error!("[COORD] Invalid topography data for shard: ({},{},{},{})", 
                            shard.x, shard.y, shard.width, shard.height);
                    },
                    _ => {
                        log_error!("[COORD] Unexpected response for topography request");
                    }
                }
            } else {
                log_error!("[COORD] Failed to receive response for topography request");
            }
        } else {
            log_error!("[COORD] Failed to connect to backend for topography request");
        }
    }

    async fn send_message<T: serde::Serialize>(stream: &mut TcpStream, msg: &T) -> Result<(), Box<dyn std::error::Error>> {
        let encoded = bincode::serialize(msg)?;
        let len = (encoded.len() as u32).to_be_bytes();
        stream.write_all(&len).await?;
        stream.write_all(&encoded).await?;
        Ok(())
    }

    async fn receive_message<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Option<T> {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            log_error!("Failed to read message length");
            return None;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        if stream.read_exact(&mut buf).await.is_err() {
            log_error!("Failed to read message body");
            return None;
        }
        bincode::deserialize(&buf).ok()
    }

    pub async fn generate_topography(&self) {
        let horizontal_count = self.info.total_width / self.info.shard_width;
        let vertical_count = self.info.total_height / self.info.shard_height;
        let shard_w = self.info.shard_width;
        let shard_h = self.info.shard_height;
        let default_value = self.info.default_value;
        let points_per_subgrid = self.info.points_per_subgrid;
        let (min_value, max_value) = self.info.points_min_max_value;

        // Border caches to ensure adjacent shards share borders
        let mut top_borders: Vec<Vec<u8>> = vec![vec![0; shard_w]; horizontal_count];
        let mut left_borders: Vec<Vec<u8>> = vec![vec![0; shard_h]; vertical_count];

        // Generate all horizontal borders (top for each row)
        for x in 0..horizontal_count {
            top_borders[x] = Self::generate_border(shard_w, default_value);
        }
        // Generate all vertical borders (left for each column)
        for y in 0..vertical_count {
            left_borders[y] = Self::generate_border(shard_h, default_value);
        }

        for y in 0..vertical_count {
            for x in 0..horizontal_count {
                let shard = Shard {
                    x: (x * shard_w) as i32,
                    y: (y * shard_h) as i32,
                    width: shard_w as i32,
                    height: shard_h as i32,
                };

                // Top border: from cache or generate
                let top_border = if y == 0 {
                    Self::generate_border(shard_w, default_value)
                } else {
                    // Bottom border of the shard above
                    Self::clone_border(&top_borders[x])
                };
                // Left border: from cache or generate
                let left_border = if x == 0 {
                    Self::generate_border(shard_h, default_value)
                } else {
                    // Right border of the shard to the left
                    Self::clone_border(&left_borders[y])
                };

                // Right border: generate and cache for next shard
                let right_border = if x + 1 < horizontal_count {
                    let border = Self::generate_border(shard_h, default_value);
                    left_borders[y] = border.clone();
                    border
                } else {
                    Self::generate_border(shard_h, default_value)
                };
                // Bottom border: generate and cache for next row
                let bottom_border = if y + 1 < vertical_count {
                    let border = Self::generate_border(shard_w, default_value);
                    top_borders[x] = border.clone();
                    border
                } else {
                    Self::generate_border(shard_w, default_value)
                };

                // Generate random points inside the shard (excluding borders)
                let points = Self::generate_points(
                    x,
                    y,
                    shard_w,
                    shard_h,
                    points_per_subgrid,
                    min_value,
                    max_value,
                );

                let topography = ShardTopographyInfo {
                    default_value,
                    top_border,
                    bottom_border,
                    left_border,
                    right_border,
                    points,
                };

                self.send_topography_to_local_shard(shard, topography).await;
            }
        }
    }

    // Helper: Generate a border with linear interpolation and ±1 step
    fn generate_border(len: usize, default_value: u8) -> Vec<u8> {
        if len == 0 { return vec![]; }
        let mut rng = rand::thread_rng();
        let start = default_value;
        let end = default_value;
        let mut border = vec![0u8; len];
        border[0] = start;
        border[len - 1] = end;
        for i in 1..len - 1 {
            let prev = border[i - 1] as i16;
            let step = rng.gen_range(-1..=1);
            let mut val = prev + step;
            if val < 0 { val = 0; }
            if val > 255 { val = 255; }
            border[i] = val as u8;
        }
        border
    }

    // Helper: Clone a border
    fn clone_border(border: &Vec<u8>) -> Vec<u8> {
        border.clone()
    }

    // Helper: Generate random points inside the shard (excluding borders)
    fn generate_points(
        shard_x: usize,
        shard_y: usize,
        shard_w: usize,
        shard_h: usize,
        points_per_subgrid: u8,
        min_value: u8,
        max_value: u8,
    ) -> Vec<(u16, u16, u8)> {
        let mut points = Vec::new();
        let mut rng = StdRng::seed_from_u64((shard_x as u64) << 32 | (shard_y as u64));
        for _ in 0..points_per_subgrid {
            let x = rng.gen_range(1..(shard_w as u16 - 1));
            let y = rng.gen_range(1..(shard_h as u16 - 1));
            let value = rng.gen_range(min_value..=max_value);
            points.push((x, y, value));
        }
        points
    }
}  