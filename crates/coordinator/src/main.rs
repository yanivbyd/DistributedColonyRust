mod init_colony;
mod global_topography;

use shared::coordinator_api::CoordinatorRequest;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use shared::coordinator_api::{COORDINATOR_PORT };
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error, log};
use bincode;
use tokio::task;

use crate::init_colony::initialize_colony;

async fn handle_client(socket: TcpStream) {
    log!("[COORD] handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log!("[COORD] handle_client: received bytes");
        match bincode::deserialize::<CoordinatorRequest>(&bytes) {
            Ok(_request) => {
                // TODO: Handle different request types
                // CoordinatorResponse is an empty enum, so we can't create an instance
                // For now, we'll just log the request and continue
                log!("[COORD] Received request: {:?}", _request);
            },
            Err(e) => {
                log_error!("[COORD] Failed to deserialize CoordinatorRequest: {}", e);
                continue;
            }
        }
    }
    log!("[COORD] handle_client: connection closed");
}

#[tokio::main]
async fn main() {
    init_logging("output/logs/coordinator.log");
    log_startup("COORDINATOR");
    set_panic_hook();
    
    // Make initialize_colony blocking by using spawn_blocking
    task::spawn_blocking(|| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
        rt.block_on(initialize_colony())
    }).await.expect("Failed to initialize colony");

    let addr = format!("127.0.0.1:{}", COORDINATOR_PORT);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    log!("[COORD] Listening on {}", addr);

    loop {
        log!("[COORD] Waiting for connection...");
        match listener.accept().await {
            Ok((socket, _)) => {
                log!("[COORD] Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("[COORD] Connection failed: {}", e),
        }
    }
} 