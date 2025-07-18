// Responsible for calling other backend servers (BEs) for shard synchronization and communication.

use tokio::net::TcpStream;
use shared::be_api::{BACKEND_PORT, BackendRequest, BackendResponse, UpdatedShardContentsRequest};
use bincode;
use shared::{log, log_error};
use tokio::sync::Mutex;
use std::sync::OnceLock;
use std::sync::Arc;

static SELF_STREAM: OnceLock<Arc<Mutex<Option<TcpStream>>>> = OnceLock::new();

async fn get_self_stream() -> Arc<Mutex<Option<TcpStream>>> {
    SELF_STREAM.get_or_init(|| Arc::new(Mutex::new(None))).clone()
}

/// Ensures a connected TcpStream to self, reconnecting if needed.
async fn ensure_self_stream() -> Option<TcpStream> {
    let stream_mutex = get_self_stream().await;
    let mut guard = stream_mutex.lock().await;
    let need_reconnect = guard.is_none() || guard.as_ref().map(|s| s.peer_addr().is_err()).unwrap_or(true);
    if need_reconnect {
        let addr = format!("127.0.0.1:{}", BACKEND_PORT);
        match TcpStream::connect(addr).await {
            Ok(stream) => {
                *guard = Some(stream);
            }
            Err(e) => {
                log_error!("[BE-BE] Failed to connect to self for ping: {}", e);
                *guard = None;
                return None;
            }
        }
    }
    guard.take()
}

pub async fn broadcast_shard_contents_exported(exported: UpdatedShardContentsRequest) {
    let mut stream_opt = ensure_self_stream().await;
    if let Some(mut stream) = stream_opt.as_mut() {
        let backend_req = BackendRequest::UpdatedShardContents(exported);
        match send_be_request(&mut stream, &backend_req).await {
            Ok(_) => {
                let x = backend_req.get_shard_x();
                let y = backend_req.get_shard_y();
                let w = backend_req.get_shard_width();
                let h = backend_req.get_shard_height();
                log!("[BE-BE] Sent shard contents to self ({},{},{},{})", x, y, w, h);
            }
            Err(e) => {
                log_error!("[BE-BE] Failed to send to self: {}", e);
            }
        }
    } else {
        log_error!("[BE-BE] Could not get or connect self stream for broadcast");
    }
}

async fn send_be_request(stream: &mut TcpStream, req: &BackendRequest) -> Result<BackendResponse, String> {
    use tokio::io::{AsyncWriteExt, AsyncReadExt};
    let encoded = bincode::serialize(req).map_err(|e| format!("serialize: {}", e))?;
    let len = (encoded.len() as u32).to_be_bytes();
    stream.write_all(&len).await.map_err(|e| format!("write len: {}", e))?;
    stream.write_all(&encoded).await.map_err(|e| format!("write body: {}", e))?;
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await.map_err(|e| format!("read len: {}", e))?;
    let resp_len = u32::from_be_bytes(len_buf) as usize;
    let mut resp_buf = vec![0u8; resp_len];
    stream.read_exact(&mut resp_buf).await.map_err(|e| format!("read body: {}", e))?;
    bincode::deserialize::<BackendResponse>(&resp_buf).map_err(|e| format!("deserialize: {}", e))
}

/// Pings the backend itself and logs the result.
pub async fn ping_be() {
    let stream_mutex = get_self_stream().await;
    let mut stream_opt = ensure_self_stream().await;
    let mut remove_stream = false;
    if let Some(mut stream) = stream_opt.as_mut() {
        match send_be_request(&mut stream, &BackendRequest::Ping).await {
            Ok(BackendResponse::Ping) => log!("[BE-BE] Ping successful"),
            Ok(other) => log_error!("[BE-BE] Unexpected ping response: {:?}", other),
            Err(e) => {
                log_error!("[BE-BE] Ping failed: {}", e);
                remove_stream = true;
            }
        }
        if !remove_stream {
            let mut guard = stream_mutex.lock().await;
            *guard = Some(stream_opt.take().unwrap());
        }
    } else {
        log_error!("[BE-BE] Could not get or connect self stream");
    }
    if remove_stream {
        let mut guard = stream_mutex.lock().await;
        *guard = None;
    }
} 

// Helper methods for logging (add below):
trait BackendRequestShardInfo {
    fn get_shard_x(&self) -> i32;
    fn get_shard_y(&self) -> i32;
    fn get_shard_width(&self) -> i32;
    fn get_shard_height(&self) -> i32;
}

impl BackendRequestShardInfo for BackendRequest {
    fn get_shard_x(&self) -> i32 {
        match self {
            BackendRequest::UpdatedShardContents(req) => req.updated_shard.x,
            _ => 0,
        }
    }
    fn get_shard_y(&self) -> i32 {
        match self {
            BackendRequest::UpdatedShardContents(req) => req.updated_shard.y,
            _ => 0,
        }
    }
    fn get_shard_width(&self) -> i32 {
        match self {
            BackendRequest::UpdatedShardContents(req) => req.updated_shard.width,
            _ => 0,
        }
    }
    fn get_shard_height(&self) -> i32 {
        match self {
            BackendRequest::UpdatedShardContents(req) => req.updated_shard.height,
            _ => 0,
        }
    }
} 