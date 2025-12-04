use shared::{log, log_error};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use crate::cloud_start::cloud_start_colony;
use shared::ssm;
use std::fmt::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub const HTTP_SERVER_PORT: u16 = 8084;
const HTTP_BIND_HOST: &str = "0.0.0.0";

static CLOUD_START_IN_PROGRESS: Mutex<bool> = Mutex::const_new(false);
static CLOUD_START_SUCCESS: AtomicBool = AtomicBool::new(false);
static CLOUD_START_COMPLETED: AtomicBool = AtomicBool::new(false);
static CLOUD_START_TIMESTAMP: Mutex<Option<u64>> = Mutex::const_new(None);

fn build_http_bind_addr() -> String {
    format!("{}:{}", HTTP_BIND_HOST, HTTP_SERVER_PORT)
}

pub async fn start_http_server() {
    let addr = build_http_bind_addr();
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind HTTP server");
    log!("HTTP server listening on {}", addr);
    
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                tokio::spawn(async move {
                    let mut buffer = [0; 1024];
                    if let Ok(n) = stream.read(&mut buffer).await {
                        let request = String::from_utf8_lossy(&buffer[..n]);
                        
                        if request.starts_with("POST /cloud-start") {
                            let mut in_progress = CLOUD_START_IN_PROGRESS.lock().await;
                            if *in_progress {
                                let response = "HTTP/1.1 409 Conflict\r\nContent-Length: 23\r\n\r\nCloud-start in progress";
                                let _ = stream.write_all(response.as_bytes()).await;
                            } else {
                                *in_progress = true;
                                CLOUD_START_COMPLETED.store(false, Ordering::Release);
                                CLOUD_START_SUCCESS.store(false, Ordering::Release);
                                {
                                    let mut timestamp = CLOUD_START_TIMESTAMP.lock().await;
                                    *timestamp = Some(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs());
                                }
                                drop(in_progress);
                                
                                log!("Received cloud-start request via HTTP");
                                tokio::spawn(async {
                                    let result = cloud_start_colony().await;
                                    let mut in_progress = CLOUD_START_IN_PROGRESS.lock().await;
                                    *in_progress = false;
                                    CLOUD_START_COMPLETED.store(true, Ordering::Release);
                                    CLOUD_START_SUCCESS.store(result, Ordering::Release);
                                });
                                
                                let response = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n";
                                let _ = stream.write_all(response.as_bytes()).await;
                            }
                        } else if request.starts_with("GET /cloud-start-status") {
                            let completed = CLOUD_START_COMPLETED.load(Ordering::Acquire);
                            let success = CLOUD_START_SUCCESS.load(Ordering::Acquire);
                            let _timestamp = CLOUD_START_TIMESTAMP.lock().await;
                            
                            let status_text = if !completed {
                                "in_progress"
                            } else if success {
                                "success"
                            } else {
                                "failed"
                            };
                            
                            let body = format!("status: {}\n", status_text);
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else if request.starts_with("GET /debug-network") {
                            let body = render_network_info().await;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else if request.starts_with("GET /debug-ssm") {
                            let body = render_ssm_state().await;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else if request.starts_with("GET /cloud-start") || request.starts_with("GET /") {
                            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nCloud-start API";
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else {
                            let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
                            let _ = stream.write_all(response.as_bytes()).await;
                        }
                    }
                });
            }
            Err(e) => log_error!("HTTP connection failed: {}", e),
        }
    }
}

async fn render_ssm_state() -> String {
    let mut body = String::new();

    match ssm::discover_coordinator().await {
        Some(addr) => {
            let _ = writeln!(body, "Coordinator: {}", addr.to_address());
        }
        None => {
            let _ = writeln!(body, "Coordinator: <none>");
        }
    }

    let backends = ssm::discover_backends().await;
    if backends.is_empty() {
        let _ = writeln!(body, "Backends: <none>");
    } else {
        let _ = writeln!(body, "Backends ({} total):", backends.len());
        for (idx, backend) in backends.iter().enumerate() {
            let _ = writeln!(body, "  {}. {}", idx + 1, backend.to_address());
        }
    }

    body
}

async fn render_network_info() -> String {
    let mut body = String::new();
    
    // Get coordinator IP from SSM
    match ssm::discover_coordinator().await {
        Some(addr) => {
            let _ = writeln!(body, "Coordinator IP (SSM): {}", addr.ip);
            let _ = writeln!(body, "Coordinator Port: {}", addr.port);
        }
        None => {
            let _ = writeln!(body, "Coordinator IP (SSM): <not registered>");
        }
    }
    
    // Try to get local network interfaces (basic info)
    let _ = writeln!(body, "");
    let _ = writeln!(body, "Network Diagnostics:");
    let _ = writeln!(body, "  Listening on: 0.0.0.0:{} (coordinator protocol)", crate::http_server::HTTP_SERVER_PORT - 1);
    let _ = writeln!(body, "  HTTP server: 0.0.0.0:{}", crate::http_server::HTTP_SERVER_PORT);
    
    body
}

