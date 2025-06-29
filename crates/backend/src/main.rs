use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use shared::BACKEND_PORT;

async fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 512];
    match stream.read(&mut buffer).await {
        Ok(_) => {
            println!("[BE] Received a connection");
            let _ = stream.write_all(b"Hello from backend!\n").await;
        }
        Err(e) => println!("[BE] Failed to read from client: {}", e),
    }
}

#[tokio::main]
async fn main() {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let listener = TcpListener::bind(&addr).await.expect("Could not bind");
    println!("[BE] Listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_client(stream));
            }
            Err(e) => println!("[BE] Connection failed: {}", e),
        }
    }
} 