use shared::{BACKEND_PORT, BackendRequest, BackendResponse, InitColonyRequest, CLIENT_TIMEOUT};
use bincode;
use std::net::TcpStream;
use std::io::{Read, Write};

fn main() {
    let mut stream = connect_to_backend();

    send_ping(&mut stream);
    send_init_colony(&mut stream);
}

fn connect_to_backend() -> TcpStream {
    let addr = format!("127.0.0.1:{}", BACKEND_PORT);
    let stream = TcpStream::connect(&addr).expect("Failed to connect to backend");
    stream
        .set_read_timeout(Some(CLIENT_TIMEOUT))
        .expect("set_read_timeout call failed");
    stream
}

fn send_ping(stream: &mut TcpStream) {
    let ping = BackendRequest::Ping;
    send_message(stream, &ping);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::Ping => println!("[FO] Received PingResponse"),
            _ => println!("[FO] Unexpected response"),
        }
    }
}

fn send_init_colony(stream: &mut TcpStream) {
    let init = BackendRequest::InitColony(InitColonyRequest { width: 500, height: 500 });
    send_message(stream, &init);

    if let Some(response) = receive_message::<BackendResponse>(stream) {
        match response {
            BackendResponse::InitColony => println!("[FO] Received InitColony response"),
            _ => println!("[FO] Unexpected response"),
        }
    }
}

// Helper to send a length-prefixed message
fn send_message<T: serde::Serialize>(stream: &mut TcpStream, msg: &T) {
    let encoded = bincode::serialize(msg).expect("Failed to serialize message");
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).expect("Failed to write length");
    stream.write_all(&encoded).expect("Failed to write message");
}

// Helper to receive a length-prefixed message
fn receive_message<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Option<T> {
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).is_err() {
        println!("[FO] Failed to read message length");
        return None;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    if stream.read_exact(&mut buf).is_err() {
        println!("[FO] Failed to read message body");
        return None;
    }
    bincode::deserialize(&buf).ok()
} 