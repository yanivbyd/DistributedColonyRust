use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, GetSubImageResponse};
use bincode;
use shared::logging::{log_startup, init_logging, set_panic_hook};
use shared::{log, log_error};

mod colony;
mod ticker;
#[allow(dead_code)]
use colony::ColonyShard;

async fn handle_client(socket: tokio::net::TcpStream) {
    log!("[BE] handle_client: new connection");
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        log!("[BE] handle_client: received bytes");
        // Try to deserialize a BackendRequest
        match bincode::deserialize::<BackendRequest>(&bytes) {
            Ok(BackendRequest::Ping) => {
                let response = BackendResponse::Ping;
                let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
                if let Err(e) = framed.send(encoded.into()).await {
                    log_error!("[BE] Failed to send PingResponse: {}", e);
                }
            }
            Ok(BackendRequest::InitColony(req)) => {
                if !ColonyShard::is_initialized() {
                    ColonyShard::init_colony(&req);
                }
                let response = BackendResponse::InitColony;
                let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
                if let Err(e) = framed.send(encoded.into()).await {
                    log_error!("[BE] Failed to send InitColony response: {}", e);
                }
            }
            Ok(BackendRequest::GetSubImage(req)) => {
                log!("[BE] GetSubImage request: x={}, y={}, w={}, h={}", req.x, req.y, req.width, req.height);
                let image = ColonyShard::instance().get_sub_image(&req);
                let response = BackendResponse::GetSubImage(GetSubImageResponse { colors: image });
                let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
                if let Err(e) = framed.send(encoded.into()).await {
                    log_error!("[BE] Failed to send GetSubImage response: {}", e);
                } else {
                    log!("[BE] Sent GetSubImage response");
                }
            }
            Err(e) => {
                log_error!("[BE] Failed to deserialize BackendRequest: {}", e);
            }
        }
    }
    log!("[BE] handle_client: connection closed");
}

#[tokio::main]
async fn main() {
    init_logging("output/logs/be.log");
    log_startup("BE");
    set_panic_hook();
    shared::metrics::start_metrics_endpoint();
    ticker::start_ticker();
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    log!("[BE] Listening on {}", addr);

    loop {
        log!("[BE] Waiting for connection...");
        match listener.accept().await {
            Ok((socket, _)) => {
                log!("[BE] Accepted connection");
                tokio::spawn(handle_client(socket));
            }
            Err(e) => log_error!("[BE] Connection failed: {}", e),
        }
    }
} 