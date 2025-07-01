use tokio::net::TcpListener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_stream::StreamExt;
use futures_util::SinkExt;
use shared::{BACKEND_PORT, BackendRequest, BackendResponse};
use bincode;
mod colony;
#[allow(dead_code)]
use colony::ColonySubGrid;

async fn handle_client(socket: tokio::net::TcpStream) {
    let mut framed = Framed::new(socket, LengthDelimitedCodec::new());
    while let Some(Ok(bytes)) = framed.next().await {
        // Try to deserialize a BackendRequest
        match bincode::deserialize::<BackendRequest>(&bytes) {
            Ok(BackendRequest::Ping) => {
                println!("[BE] Received PingRequest");
                let response = BackendResponse::Ping;
                let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
                if let Err(e) = framed.send(encoded.into()).await {
                    println!("[BE] Failed to send PingResponse: {}", e);
                }
            }
            Ok(BackendRequest::InitColony(req)) => {
                println!("[BE] Received InitColonyRequest: width={}, height={}", req.width, req.height);
                ColonySubGrid::init_colony(&req);
                let response = BackendResponse::InitColony;
                let encoded = bincode::serialize(&response).expect("Failed to serialize BackendResponse");
                if let Err(e) = framed.send(encoded.into()).await {
                    println!("[BE] Failed to send InitColony response: {}", e);
                }
            }
            Err(e) => {
                println!("[BE] Failed to deserialize BackendRequest: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    println!("[BE] Listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok((socket, _)) => {
                tokio::spawn(handle_client(socket));
            }
            Err(e) => println!("[BE] Connection failed: {}", e),
        }
    }
} 