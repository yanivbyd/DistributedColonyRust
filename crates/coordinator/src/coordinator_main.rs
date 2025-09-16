mod init_colony;
mod global_topography;
mod coordinator_storage;
mod coordinator_context;
mod coordinator_ticker;
mod backend_client;
mod tick_monitor;
mod colony_event_generator;

use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse, RoutingEntry};
use shared::cluster_topology::ClusterTopology;
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
    let events = context.get_colony_events();
    let mut events_clone = events.clone();
    drop(events); // Release the lock
    
    // Sort by tick in descending order (most recent first)
    events_clone.sort_by(|a, b| b.tick.cmp(&a.tick));
    
    // Take only the top K events
    let limited_events = events_clone.into_iter().take(limit).collect();
    
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
            Err(e) => {
                log_error!("Failed to deserialize CoordinatorRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
    log!("handle_client: connection closed");
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