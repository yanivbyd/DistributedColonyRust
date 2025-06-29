use shared::{BACKEND_PORT, BackendRequest, BackendResponse, PingRequest};
use bincode;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;

#[tokio::main]
async fn main() {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    match TcpStream::connect(&addr).await {
        Ok(socket) => {
            println!("[FO] Connected to backend at {}", addr);
            let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
            // Send BackendRequest::Ping
            let ping = BackendRequest::Ping(PingRequest);
            let encoded = bincode::serialize(&ping).expect("Failed to serialize BackendRequest");
            if let Err(e) = framed.send(encoded.into()).await {
                println!("[FO] Failed to send PingRequest: {}", e);
                return;
            }
            // Wait for BackendResponse
            if let Some(Ok(bytes)) = framed.next().await {
                match bincode::deserialize::<BackendResponse>(&bytes) {
                    Ok(BackendResponse::Ping(_pong)) => println!("[FO] Received PingResponse"),
                    Err(e) => println!("[FO] Failed to deserialize BackendResponse: {}", e),
                }
            } else {
                println!("[FO] No response from backend");
            }
        }
        Err(e) => {
            println!("[FO] Failed to connect to backend: {}", e);
        }
    }
} 