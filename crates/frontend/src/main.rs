use shared::{BACKEND_PORT, CLIENT_TIMEOUT, BackendRequest, BackendResponse};
use bincode;
use tokio::net::TcpStream;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use tokio::time;

#[tokio::main]
async fn main() {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    match TcpStream::connect(&addr).await {
        Ok(socket) => {
            println!("[FO] Connected to backend at {}", addr);
            let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
            // Send BackendRequest::Ping
            let ping = BackendRequest::Ping;
            let encoded = bincode::serialize(&ping).expect("Failed to serialize BackendRequest");
            if let Err(e) = framed.send(encoded.into()).await {
                println!("[FO] Failed to send PingRequest: {}", e);
                return;
            }
            // Wait for BackendResponse with timeout
            let response = time::timeout(CLIENT_TIMEOUT, framed.next()).await;
            match response {
                Ok(Some(Ok(bytes))) => {
                    match bincode::deserialize::<BackendResponse>(&bytes) {
                        Ok(BackendResponse::Ping) => println!("[FO] Received PingResponse"),
                        Err(e) => println!("[FO] Failed to deserialize BackendResponse: {}", e),
                    }
                }
                Ok(Some(Err(e))) => {
                    println!("[FO] Error reading response: {}", e);
                }
                Ok(None) => {
                    println!("[FO] Connection closed by server");
                }
                Err(_) => {
                    println!("[FO] Timed out waiting for server response");
                }
            }
        }
        Err(e) => {
            println!("[FO] Failed to connect to backend: {}", e);
        }
    }
} 