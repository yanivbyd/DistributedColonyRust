mod init_colony;
mod global_topography;
mod coordinator_storage;
mod coordinator_ticker;
mod backend_client;
mod tick_monitor;

use shared::coordinator_api::{CoordinatorRequest, CoordinatorResponse, RoutingEntry};
use shared::colony_model::Shard;
use shared::be_api::BACKEND_PORT;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use shared::coordinator_api::{COORDINATOR_PORT };
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error, log};
use bincode;
use tokio::task;
use futures_util::SinkExt;

use crate::init_colony::initialize_colony;

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

fn call_label(response: &CoordinatorResponse) -> &'static str {
    match response {
        CoordinatorResponse::GetRoutingTableResponse { .. } => "GetRoutingTable",
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
    const WIDTH_IN_SHARDS: i32 = 5;
    const HEIGHT_IN_SHARDS: i32 = 3;
    const SHARD_WIDTH: i32 = 250;
    const SHARD_HEIGHT: i32 = 250;
    
    let mut entries = Vec::new();
    for y in 0..HEIGHT_IN_SHARDS {
        for x in 0..WIDTH_IN_SHARDS {
            entries.push(RoutingEntry {
                shard: Shard {
                    x: x * SHARD_WIDTH,
                    y: y * SHARD_HEIGHT,
                    width: SHARD_WIDTH,
                    height: SHARD_HEIGHT,
                },
                hostname: "127.0.0.1".to_string(),
                port: BACKEND_PORT,
            });
        }
    }

    CoordinatorResponse::GetRoutingTableResponse { entries }
}

async fn handle_client(socket: TcpStream) {
    log!("handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log!("handle_client: received bytes");
        let response = match bincode::deserialize::<CoordinatorRequest>(&bytes) {
            Ok(CoordinatorRequest::GetRoutingTable) => handle_get_routing_table().await,
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
    init_logging("output/logs/coordinator.log");
    log_startup("COORDINATOR");
    set_panic_hook();
    
    coordinator_ticker::start_coordinator_ticker();
    
    // Make initialize_colony blocking by using spawn_blocking
    task::spawn_blocking(|| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(initialize_colony())
    }).await.expect("Failed to initialize colony");

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