use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use shared::coordinator_api::{COORDINATOR_PORT, CoordinatorRequest, CoordinatorResponse, InitColonyRequest, InitColonyResponse, GetColonyInfoRequest, GetColonyInfoResponse};
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log_error, log};
use bincode;

type FramedStream = Framed<TcpStream, LengthDelimitedCodec>;

fn call_label(response: &CoordinatorResponse) -> &'static str {
    match response {
        CoordinatorResponse::InitColony(_) => "InitColony",
        CoordinatorResponse::GetColonyInfo(_) => "GetColonyInfo",
    }
}

async fn send_response(framed: &mut FramedStream, response: CoordinatorResponse) {
    let encoded = bincode::serialize(&response).expect("Failed to serialize CoordinatorResponse");
    let label = call_label(&response);
    if let Err(e) = framed.send(encoded.into()).await {
        log_error!("[COORD] Failed to send {} response: {}", label, e);
    } else {
        log!("[COORD] Sent {} response", label);
    }
}

async fn handle_client(socket: TcpStream) {
    log!("[COORD] handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log!("[COORD] handle_client: received bytes");
        let response = match bincode::deserialize::<CoordinatorRequest>(&bytes) {
            Ok(CoordinatorRequest::InitColony(req)) => handle_init_colony(req).await,
            Ok(CoordinatorRequest::GetColonyInfo(req)) => handle_get_colony_info(req).await,
            Err(e) => {
                log_error!("[COORD] Failed to deserialize CoordinatorRequest: {}", e);
                continue;
            }
        };
        send_response(&mut framed, response).await;
    }
    log!("[COORD] handle_client: connection closed");
}

async fn handle_init_colony(req: InitColonyRequest) -> CoordinatorResponse {
    // TODO: Implement actual colony initialization logic
    log!("[COORD] InitColony request: width={}, height={}", req.width, req.height);
    CoordinatorResponse::InitColony(InitColonyResponse::Ok)
}

async fn handle_get_colony_info(_req: GetColonyInfoRequest) -> CoordinatorResponse {
    // TODO: Implement actual colony info retrieval logic
    log!("[COORD] GetColonyInfo request");
    CoordinatorResponse::GetColonyInfo(GetColonyInfoResponse::ColonyNotInitialized)
}

#[tokio::main]
async fn main() {
    init_logging("output/logs/coordinator.log");
    log_startup("COORDINATOR");
    set_panic_hook();
    
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