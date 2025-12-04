use std::net::TcpStream;
use std::io::{Read, Write};
use std::sync::OnceLock;
use std::time::Duration;
use bincode;
use tokio::net::TcpStream as TokioTcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use backoff::{ExponentialBackoff, Error as BackoffError};
use crate::connection_pool::AsyncConnectionPool;
use crate::cluster_topology::HostInfo;

static CONNECTION_POOL: OnceLock<AsyncConnectionPool> = OnceLock::new();

fn get_connection_pool() -> &'static AsyncConnectionPool {
    CONNECTION_POOL.get_or_init(|| AsyncConnectionPool::new())
}

pub async fn send_request_with_pool<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
    host_info: &HostInfo,
    request: &Req
) -> Result<Resp, Box<dyn std::error::Error + Send + Sync>> {
    let pool = get_connection_pool();
    let conn_info = pool.get_connection(host_info).await.ok_or("Failed to get connection")?;
    let mut conn = conn_info.lock().await;
    
    // Get the stream, creating a new connection if needed
    let stream = if let Some(ref mut stream) = conn.stream {
        stream
    } else {
        // Recreate connection if it was closed (with retry and backoff)
        let addr = host_info.to_address();
        let new_stream = connect_with_backoff(&addr).await?;
        conn.stream = Some(new_stream);
        conn.is_healthy = true;
        conn.stream.as_mut().unwrap()
    };
    
    // Send request and receive response
    let response = send_request_and_receive_response_async(stream, request).await?;
    
    // Update last used time
    conn.last_used = std::time::Instant::now();
    
    Ok(response)
}

pub fn send_request<T: serde::Serialize>(stream: &mut TcpStream, request: &T) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = bincode::serialize(request)?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(&encoded)?;
    Ok(())
}

pub fn receive_response<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Result<T, Box<dyn std::error::Error>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf)?;
    let response = bincode::deserialize(&buf)?;
    Ok(response)
}

pub async fn send_request_async<T: serde::Serialize>(stream: &mut TokioTcpStream, request: &T) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let encoded = bincode::serialize(request)?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&encoded).await?;
    Ok(())
}

pub async fn receive_response_async<T: serde::de::DeserializeOwned>(stream: &mut TokioTcpStream) -> Result<T, Box<dyn std::error::Error + Send + Sync>> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; resp_len];
    stream.read_exact(&mut buf).await?;
    let response = bincode::deserialize(&buf)?;
    Ok(response)
}

pub async fn send_request_and_receive_response_async<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
    stream: &mut TokioTcpStream, 
    request: &Req
) -> Result<Resp, Box<dyn std::error::Error + Send + Sync>> {
    send_request_async(stream, request).await?;
    receive_response_async(stream).await
}

/// Connect to a backend with exponential backoff retry
async fn connect_with_backoff(addr: &str) -> Result<TokioTcpStream, std::io::Error> {
    // Configure exponential backoff: start with 100ms, max 2s, max 10s total
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(2),
        max_elapsed_time: Some(Duration::from_secs(10)),
        multiplier: 2.0,
        ..Default::default()
    };
    
    let operation = || async {
        match TokioTcpStream::connect(addr).await {
            Ok(stream) => Ok(stream),
            Err(e) => {
                crate::log_error!("Failed to connect to backend at {}: {}", addr, e);
                Err(BackoffError::transient(e))
            }
        }
    };
    
    backoff::future::retry(backoff, operation).await
}
