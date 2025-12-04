use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, Duration};
use tokio::sync::Mutex;
use tokio::net::TcpStream;
use backoff::{ExponentialBackoff, Error as BackoffError};
use crate::cluster_topology::HostInfo;

#[derive(Clone)]
pub struct AsyncConnectionPool {
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<AsyncConnectionInfo>>>>>,
}

#[derive(Debug)]
pub struct AsyncConnectionInfo {
    pub stream: Option<TcpStream>,
    pub last_used: Instant,
    pub is_healthy: bool,
    pub host_info: HostInfo,
}

impl AsyncConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_connection(&self, host_info: &HostInfo) -> Option<Arc<Mutex<AsyncConnectionInfo>>> {
        let addr = host_info.to_address();
        let mut connections = self.connections.lock().await;
        
        // Check if we have an existing connection
        if let Some(conn_info) = connections.get(&addr) {
            let mut conn = conn_info.lock().await;
            conn.last_used = Instant::now();
            
            // Check if connection is still healthy
            if conn.is_healthy && conn.stream.is_some() {
                return Some(conn_info.clone());
            }
        }
        
        // Create new connection with exponential backoff retry
        let stream = match connect_with_backoff(&addr).await {
            Ok(stream) => Some(stream),
            Err(_) => None,
        };
        
        let is_healthy = stream.is_some();
        let conn_info = Arc::new(Mutex::new(AsyncConnectionInfo {
            stream,
            last_used: Instant::now(),
            is_healthy,
            host_info: host_info.clone(),
        }));
        
        connections.insert(addr, conn_info.clone());
        Some(conn_info)
    }

    pub async fn cleanup_stale_connections(&self) {
        let mut connections = self.connections.lock().await;
        let now = Instant::now();
        
        connections.retain(|_, conn_info| {
            let conn = conn_info.try_lock();
            if let Ok(conn) = conn {
                // Keep connections that are less than 30 seconds old
                now.duration_since(conn.last_used).as_secs() < 30
            } else {
                true // Keep if we can't lock (might be in use)
            }
        });
    }
}

/// Connect to a backend with exponential backoff retry
async fn connect_with_backoff(addr: &str) -> Result<TcpStream, std::io::Error> {
    // Configure exponential backoff: start with 100ms, max 2s, max 10s total
    let backoff = ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(2),
        max_elapsed_time: Some(Duration::from_secs(10)),
        multiplier: 2.0,
        ..Default::default()
    };
    
    let operation = || async {
        match TcpStream::connect(addr).await {
            Ok(stream) => Ok(stream),
            Err(e) => {
                crate::log_error!("Failed to connect to backend at {}: {}", addr, e);
                Err(BackoffError::transient(e))
            }
        }
    };
    
    backoff::future::retry(backoff, operation).await
}
