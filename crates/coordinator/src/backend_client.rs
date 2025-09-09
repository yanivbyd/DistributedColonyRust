use shared::log;
use shared::be_api::{BackendRequest, BackendResponse, GetShardCurrentTickRequest, GetShardCurrentTickResponse, ApplyEventRequest, ApplyEventResponse, GetColonyInfoRequest, GetColonyInfoResponse, BACKEND_PORT};
use shared::colony_events::ColonyEvent;
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

fn get_unique_backends() -> Vec<(String, u16)> {
    const WIDTH_IN_SHARDS: i32 = 5;
    const HEIGHT_IN_SHARDS: i32 = 3;
    
    let mut backends = std::collections::HashSet::new();
    
    // Currently all shards map to the same backend, but this structure supports multiple backends
    for _y in 0..HEIGHT_IN_SHARDS {
        for _x in 0..WIDTH_IN_SHARDS {
            backends.insert(("127.0.0.1".to_string(), BACKEND_PORT));
        }
    }
    
    backends.into_iter().collect()
}

pub fn broadcast_event_to_backends(event: ColonyEvent) -> bool {
    let backends = get_unique_backends();
    let mut success_count = 0;
    let total_count = backends.len();
    
    for (hostname, port) in backends {
        let addr = format!("{}:{}", hostname, port);
        let mut stream = match TcpStream::connect(&addr) {
            Ok(stream) => stream,
            Err(e) => {
                log!("Failed to connect to backend {}: {}", addr, e);
                continue;
            }
        };
        
        let request = BackendRequest::ApplyEvent(ApplyEventRequest { event: event.clone() });
        if let Err(e) = send_request(&mut stream, &request) {
            log!("Failed to send apply event request to {}: {}", addr, e);
            continue;
        }
        
        let response: BackendResponse = match receive_response(&mut stream) {
            Ok(response) => response,
            Err(e) => {
                log!("Failed to receive apply event response from {}: {}", addr, e);
                continue;
            }
        };
        
        match response {
            BackendResponse::ApplyEvent(ApplyEventResponse::Ok) => {
                success_count += 1;
            },
            BackendResponse::ApplyEvent(ApplyEventResponse::ColonyNotInitialized) => {
                log!("Failed to apply event to {}: colony not initialized", addr);
            },
            _ => {
                log!("Unexpected response type for apply event from {}", addr);
            }
        }
    }
    
    success_count > 0
}

pub fn call_backend_get_colony_info() -> Option<(i32, i32)> {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let mut stream = TcpStream::connect(&addr).ok()?;
    
    let request = BackendRequest::GetColonyInfo(GetColonyInfoRequest);
    send_request(&mut stream, &request).ok()?;
    
    let response: BackendResponse = receive_response(&mut stream).ok()?;
    
    match response {
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::Ok { width, height, .. }) => Some((width, height)),
        BackendResponse::GetColonyInfo(GetColonyInfoResponse::ColonyNotInitialized) => {
            log!("Backend colony not initialized");
            None
        }
        _ => {
            log!("Unexpected response type for get colony info");
            None
        }
    }
}
