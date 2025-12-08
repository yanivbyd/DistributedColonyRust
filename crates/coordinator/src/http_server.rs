use shared::{log, log_error};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::cloud_start::cloud_start_colony;
use crate::coordinator_context::CoordinatorContext;
use crate::coordinator_storage::ColonyStatus;
use shared::ssm;
use std::fmt::Write;

pub const HTTP_SERVER_PORT: u16 = 8084;
const HTTP_BIND_HOST: &str = "0.0.0.0";

fn build_http_bind_addr() -> String {
    format!("{}:{}", HTTP_BIND_HOST, HTTP_SERVER_PORT)
}

fn is_colony_already_started() -> bool {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    matches!(stored_info.status, ColonyStatus::TopographyInitialized)
}

fn matches_stored_idempotency_key(key: &str) -> bool {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    stored_info.cloud_start_idempotency_key.as_ref()
        .map(|stored_key| stored_key == key)
        .unwrap_or(false)
}

fn parse_query_param(request: &str, param_name: &str) -> Option<String> {
    if let Some(query_start) = request.find('?') {
        let mut query = &request[query_start + 1..];
        if let Some(line_end) = query.find('\r') {
            query = &query[..line_end];
        } else if let Some(line_end) = query.find('\n') {
            query = &query[..line_end];
        } else if let Some(space_pos) = query.find(' ') {
            query = &query[..space_pos];
        }
        for pair in query.split('&') {
            if let Some(equal_pos) = pair.find('=') {
                let key = &pair[..equal_pos];
                let value = &pair[equal_pos + 1..];
                if key == param_name {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
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
                            let idempotency_key = parse_query_param(&request, "idempotency_key");
                            
                            if idempotency_key.is_none() {
                                let response = "HTTP/1.1 400 Bad Request\r\nContent-Length: 35\r\n\r\nidempotency_key parameter required";
                                let _ = stream.write_all(response.as_bytes()).await;
                            } else {
                                let idempotency_key = idempotency_key.unwrap();
                                
                                if is_colony_already_started() {
                                    if matches_stored_idempotency_key(&idempotency_key) {
                                        let response = "HTTP/1.1 200 OK\r\nContent-Length: 40\r\n\r\nColony already started (idempotent)";
                                        let _ = stream.write_all(response.as_bytes()).await;
                                    } else {
                                        let response = "HTTP/1.1 409 Conflict\r\nContent-Length: 26\r\n\r\nColony already started";
                                        let _ = stream.write_all(response.as_bytes()).await;
                                    }
                                } else {
                                    log!("Received cloud-start request via HTTP with idempotency_key: {}", idempotency_key);
                                    let key_clone = idempotency_key.clone();
                                    tokio::spawn(async move {
                                        cloud_start_colony(Some(key_clone)).await;
                                    });
                                    
                                    let response = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n";
                                    let _ = stream.write_all(response.as_bytes()).await;
                                }
                            }
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

