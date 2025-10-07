mod init_colony;
mod global_topography;
mod coordinator_storage;
mod coordinator_context;
mod coordinator_ticker;
mod backend_client;
mod tick_monitor;
mod colony_event_generator;

use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse, RoutingEntry, ColonyMetricStats};
use std::collections::{BTreeMap, HashMap};
use shared::cluster_topology::ClusterTopology;
use shared::be_api::StatMetric;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use shared::coordinator_api::{COORDINATOR_PORT };
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error, log};
use bincode;
use futures_util::SinkExt;

use crate::init_colony::initialize_colony;
use crate::coordinator_context::CoordinatorContext;

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

fn call_label(response: &CoordinatorResponse) -> &'static str {
    match response {
        CoordinatorResponse::GetRoutingTableResponse { .. } => "GetRoutingTable",
        CoordinatorResponse::GetColonyEventsResponse { .. } => "GetColonyEvents",
        CoordinatorResponse::GetColonyStatsResponse { .. } => "GetColonyStats",
    }
}

async fn send_response(framed: &mut FramedStream, response: CoordinatorResponse) {
    let encoded = bincode::serialize(&response).expect("Failed to serialize CoordinatorResponse");
    let label = call_label(&response);
    if let Err(e) = framed.send(encoded.into()).await {
        log_error!("Failed to send {} response: {}", label, e);
    } else {
        log!("Sent {} response", label);
    }
}

async fn handle_get_routing_table() -> CoordinatorResponse {
    let topology = ClusterTopology::get_instance();
    let mut entries = Vec::new();
    
    for shard in topology.get_all_shards() {
        let host_info = topology.get_host_for_shard(&shard).unwrap();
        entries.push(RoutingEntry {
            shard,
            hostname: host_info.hostname.clone(),
            port: host_info.port,
        });
    }

    CoordinatorResponse::GetRoutingTableResponse { entries }
}

async fn handle_get_colony_events(limit: usize) -> CoordinatorResponse {
    let context = CoordinatorContext::get_instance();
    let mut events = context.get_colony_events();
    
    // Sort by tick in descending order (most recent first)
    events.sort_by(|a, b| b.tick.cmp(&a.tick));
    
    // Take only the top K events
    let limited_events = events.into_iter().take(limit).collect();
    
    CoordinatorResponse::GetColonyEventsResponse { 
        events: limited_events
    }
}


async fn handle_client(socket: TcpStream) {
    log!("handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log!("handle_client: received bytes");
        let response = match bincode::deserialize::<CoordinatorRequest>(&bytes) {
            Ok(CoordinatorRequest::GetRoutingTable) => handle_get_routing_table().await,
            Ok(CoordinatorRequest::GetColonyEvents { limit }) => handle_get_colony_events(limit).await,
            Ok(CoordinatorRequest::GetColonyStats { metrics }) => handle_get_colony_stats(metrics).await,
            Err(e) => {
                log_error!("Failed to deserialize CoordinatorRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
    log!("handle_client: connection closed");
}

async fn handle_get_colony_stats(metrics: Vec<StatMetric>) -> CoordinatorResponse {
    // Aggregate across all shards
    let topology = ClusterTopology::get_instance();
    let shards = topology.get_all_shards();
    if shards.is_empty() {
        return CoordinatorResponse::GetColonyStatsResponse { metrics: Vec::new(), tick_count: 0 };
    }

    // Prepare index mapping for requested metrics
    fn metric_id(m: shared::be_api::StatMetric) -> u8 {
        match m {
            shared::be_api::StatMetric::Health => 0,
            shared::be_api::StatMetric::CreatureSize => 1,
            shared::be_api::StatMetric::CreateCanKill => 2,
            shared::be_api::StatMetric::CreateCanMove => 3,
            shared::be_api::StatMetric::Food => 4,
            shared::be_api::StatMetric::Age => 5,
        }
    }
    let mut pos_by_id: HashMap<u8, usize> = HashMap::new();
    for (idx, m) in metrics.iter().copied().enumerate() {
        pos_by_id.insert(metric_id(m), idx);
    }
    // counts_per_metric: per requested metric (by index) -> value -> occs
    let mut counts_per_metric: Vec<BTreeMap<i32, u64>> = vec![BTreeMap::new(); metrics.len()];

    let mut min_tick: Option<u64> = None;
    for shard in shards {
        if let Some((tick, per_metric)) = crate::backend_client::call_backend_get_shard_stats(shard, metrics.clone()) {
            min_tick = Some(match min_tick { Some(t) => t.min(tick), None => tick });
            for (metric, buckets) in per_metric {
                if let Some(&idx) = pos_by_id.get(&metric_id(metric)) {
                    let entry = counts_per_metric.get_mut(idx).unwrap();
                    for b in buckets {
                        *entry.entry(b.value).or_insert(0) += b.occs;
                    }
                }
            }
        }
    }

    // Build ordered results following the requested metrics order
    let mut results: Vec<ColonyMetricStats> = Vec::with_capacity(metrics.len());
    for (i, metric) in metrics.into_iter().enumerate() {
        let counts = std::mem::take(&mut counts_per_metric[i]);
        let mut sum: i64 = 0;
        let mut total: i64 = 0;
        for (value, occs) in &counts {
            sum += *value as i64 * *occs as i64;
            total += *occs as i64;
        }
        let avg = if total > 0 { sum as f64 / total as f64 } else { 0.0 };
        let buckets = counts.into_iter().map(|(value, occs)| shared::be_api::StatBucket { value, occs }).collect();
        results.push(ColonyMetricStats { metric, avg, buckets });
    }

    CoordinatorResponse::GetColonyStatsResponse { metrics: results, tick_count: min_tick.unwrap_or(0) }
}

#[tokio::main]
async fn main() {
    init_logging(&format!("output/logs/coordinator_{}.log", COORDINATOR_PORT));
    log_startup("COORDINATOR");
    set_panic_hook();
    
    coordinator_ticker::start_coordinator_ticker();
    
    tokio::spawn(initialize_colony()).await.expect("Failed to initialize colony");

    let addr = format!("127.0.0.1:{}", COORDINATOR_PORT);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    log!("Listening on {}", addr);

    loop {
        log!("Waiting for connection...");
        match listener.accept().await {
            Ok((socket, _)) => {
                log!("Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("Connection failed: {}", e),
        }
    }
} 