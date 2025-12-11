use shared::log;
use shared::be_api::{BackendRequest, BackendResponse, GetShardCurrentTickRequest, GetShardCurrentTickResponse, ApplyEventRequest, ApplyEventResponse, GetColonyInfoRequest, GetColonyInfoResponse, GetShardStatsRequest, GetShardStatsResponse, StatMetric};
use shared::colony_events::ColonyEvent;
use shared::colony_model::Shard as ColonyShard;
use shared::cluster_topology::ClusterTopology;
use shared::backend_communication::{send_request, receive_response};
use std::net::TcpStream;

pub fn call_backend_for_tick_count(shard: ColonyShard) -> Option<u64> {
    let topology = ClusterTopology::get_instance()?;
    let host_info = topology.get_host_for_shard(&shard)?;
    let addr = host_info.to_address();
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

pub fn call_backend_get_shard_stats(shard: ColonyShard, metrics: Vec<StatMetric>) -> Option<(u64, Vec<(StatMetric, Vec<shared::be_api::StatBucket>)>)> {
    let topology = ClusterTopology::get_instance()?;
    let host_info = topology.get_host_for_shard(&shard)?;
    let addr = host_info.to_address();
    let mut stream = TcpStream::connect(&addr).ok()?;

    let request = BackendRequest::GetShardStats(GetShardStatsRequest { shard, metrics });
    send_request(&mut stream, &request).ok()?;

    let response: BackendResponse = receive_response(&mut stream).ok()?;
    match response {
        BackendResponse::GetShardStats(GetShardStatsResponse::Ok { stats, tick_count }) => {
            // stats is Vec<ShardStatResult> for one shard; return (tick, metrics)
            let metrics = stats.first().map(|s| s.metrics.clone()).unwrap_or_default();
            Some((tick_count, metrics))
        }
        _ => None,
    }
}

fn get_unique_backends() -> Vec<(String, u16)> {
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => return Vec::new(),
    };
    topology.get_all_backend_hosts()
        .iter()
        .map(|host_info| (host_info.hostname.clone(), host_info.port))
        .collect()
}

pub fn broadcast_event_to_backends(event: ColonyEvent) -> bool {
    let backends = get_unique_backends();
    let mut success_count = 0;
    let _total_count = backends.len();
    
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
    let topology = ClusterTopology::get_instance()?;
    let backend_hosts = topology.get_all_backend_hosts();
    if backend_hosts.is_empty() {
        return None;
    }
    let host_info = &backend_hosts[0];
    let addr = host_info.to_address();
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
