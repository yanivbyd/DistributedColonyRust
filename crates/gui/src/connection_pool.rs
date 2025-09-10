use std::collections::HashMap;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use shared::cluster_topology::HostInfo;

#[derive(Clone)]
pub struct ConnectionPool {
    connections: Arc<Mutex<HashMap<String, Arc<Mutex<ConnectionInfo>>>>>,
}

#[derive(Debug)]
pub struct ConnectionInfo {
    pub stream: Option<TcpStream>,
    pub last_used: Instant,
    pub is_healthy: bool,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn get_connection(&self, host_info: &HostInfo) -> Option<Arc<Mutex<ConnectionInfo>>> {
        let addr = host_info.to_address();
        let mut connections = self.connections.lock().unwrap();
        
        // Check if we have an existing connection
        if let Some(conn_info) = connections.get(&addr) {
            let mut conn = conn_info.lock().unwrap();
            conn.last_used = Instant::now();
            
            // Check if connection is still healthy
            if conn.is_healthy && conn.stream.is_some() {
                return Some(conn_info.clone());
            }
        }
        
        // Create new connection
        let stream = match TcpStream::connect_timeout(&addr.parse().ok()?, Duration::from_millis(500)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_millis(1000))).ok()?;
                stream.set_write_timeout(Some(Duration::from_millis(500))).ok()?;
                Some(stream)
            }
            Err(_) => None,
        };
        
        let is_healthy = stream.is_some();
        let conn_info = Arc::new(Mutex::new(ConnectionInfo {
            stream,
            last_used: Instant::now(),
            is_healthy
        }));
        
        connections.insert(addr, conn_info.clone());
        Some(conn_info)
    }

}
