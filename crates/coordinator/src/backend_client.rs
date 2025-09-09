use shared::log;
use shared::be_api::{BackendRequest, BackendResponse, GetShardCurrentTickRequest, GetShardCurrentTickResponse, BACKEND_PORT};
use shared::colony_model::Shard;
use std::net::TcpStream;
use std::io::{Read, Write};
use bincode;

fn send_request<T: serde::Serialize>(stream: &mut TcpStream, request: &T) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = bincode::serialize(request)?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)?;
    Ok(())
}

fn receive_response<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Result<T, Box<dyn std::error::Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf)?;
    let response = bincode::deserialize(&buf)?;
    Ok(response)
}

pub fn call_backend_for_tick_count(shard: Shard) -> Option<u64> {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let mut stream = TcpStream::connect(&addr).ok()?;
    
    let request = BackendRequest::GetShardCurrentTick(GetShardCurrentTickRequest { shard });
    send_request(&mut stream, &request).ok()?;
    
    let response: BackendResponse = receive_response(&mut stream).ok()?;
    
    match response {
        BackendResponse::GetShardCurrentTick(GetShardCurrentTickResponse::Ok { current_tick }) => Some(current_tick),
        BackendResponse::GetShardCurrentTick(GetShardCurrentTickResponse::ColonyNotInitialized) => {
            log!("Backend colony not initialized");
            None
        }
        BackendResponse::GetShardCurrentTick(GetShardCurrentTickResponse::ShardNotAvailable) => {
            log!("Shard not available on backend");
            None
        }
        _ => {
            log!("Unexpected response type");
            None
        }
    }
}
