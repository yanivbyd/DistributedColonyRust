use shared::{log, log_error};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::colony_start::colony_start_colony;
use crate::coordinator_context::CoordinatorContext;
use crate::coordinator_storage::ColonyStatus;
use shared::ssm;
use shared::cluster_topology::ClusterTopology;
use shared::coordinator_api::ColonyEventDescription;
use std::fmt::Write;

const HTTP_BIND_HOST: &str = "0.0.0.0";

fn build_http_bind_addr(port: u16) -> String {
    format!("{}:{}", HTTP_BIND_HOST, port)
}

fn is_colony_already_started() -> bool {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    matches!(stored_info.status, ColonyStatus::TopographyInitialized)
}

fn matches_stored_idempotency_key(key: &str) -> bool {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    stored_info.colony_start_idempotency_key.as_ref()
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

pub async fn start_http_server(http_port: u16) {
    let addr = build_http_bind_addr(http_port);
    let listener = TcpListener::bind(&addr).await.expect("Failed to bind HTTP server");
    log!("HTTP server listening on {}", addr);
    
    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                tokio::spawn(async move {
                    let mut buffer = [0; 1024];
                    if let Ok(n) = stream.read(&mut buffer).await {
                        let request = String::from_utf8_lossy(&buffer[..n]);
                        
                        if request.starts_with("POST /colony-start") {
                            let idempotency_key = parse_query_param(&request, "idempotency_key");
                            
                            if idempotency_key.is_none() {
                                let response = "HTTP/1.1 400 Bad Request\r\nContent-Length: 35\r\n\r\nidempotency_key parameter required";
                                let _ = stream.write_all(response.as_bytes()).await;
                            } else {
                                let idempotency_key = idempotency_key.unwrap();
                                
                                // Check colony status and idempotency key before spawning
                                let colony_started = is_colony_already_started();
                                let idempotent_match = matches_stored_idempotency_key(&idempotency_key);
                                
                                if colony_started {
                                    if idempotent_match {
                                        let response = "HTTP/1.1 200 OK\r\nContent-Length: 40\r\n\r\nColony already started (idempotent)";
                                        let _ = stream.write_all(response.as_bytes()).await;
                                    } else {
                                        let response = "HTTP/1.1 409 Conflict\r\nContent-Length: 26\r\n\r\nColony already started";
                                        let _ = stream.write_all(response.as_bytes()).await;
                                    }
                                } else {
                                    log!("Received colony-start request via HTTP with idempotency_key: {}", idempotency_key);
                                    
                                    // Set status to Initializing before spawning async task
                                    let context = CoordinatorContext::get_instance();
                                    {
                                        let mut stored_info = context.get_coord_stored_info();
                                        stored_info.status = ColonyStatus::Initializing;
                                    }
                                    
                                    let key_clone = idempotency_key.clone();
                                    tokio::spawn(async move {
                                        colony_start_colony(Some(key_clone)).await;
                                    });
                                    
                                    let response = "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\n\r\n";
                                    let _ = stream.write_all(response.as_bytes()).await;
                                }
                            }
                        } else if request.starts_with("GET /api/colony-events") {
                            handle_get_colony_events(&mut stream, &request).await;
                        } else if request.starts_with("GET /topology") {
                            handle_get_topology(&mut stream).await;
                        } else if request.starts_with("GET /debug-ssm") {
                            let body = render_ssm_state().await;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else if request.starts_with("GET /colony-start") || request.starts_with("GET /") {
                            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nColony-start API";
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

async fn handle_get_colony_events(stream: &mut tokio::net::TcpStream, request: &str) {
    // Check if colony is initialized
    if !is_colony_already_started() {
        let error_json = r#"{"error":"Colony not initialized"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            error_json.len(),
            error_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
        return;
    }
    
    // Parse limit parameter (default 30)
    let limit = parse_query_param(request, "limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(30);
    
    // Get events using existing handler logic
    let context = CoordinatorContext::get_instance();
    let mut events = context.get_colony_events();
    
    // Sort by tick in descending order (most recent first)
    events.sort_by(|a, b| b.tick.cmp(&a.tick));
    
    // Take only the top K events
    let limited_events: Vec<ColonyEventDescription> = events.into_iter().take(limit).collect();
    
    // Build JSON response
    #[derive(serde::Serialize)]
    struct Response {
        events: Vec<ColonyEventDescription>,
    }
    
    match serde_json::to_string(&Response { events: limited_events }) {
        Ok(json) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            );
            if let Err(e) = stream.write_all(response.as_bytes()).await {
                log_error!("Failed to write colony-events response: {}", e);
            }
        }
        Err(e) => {
            let error_json = format!(r#"{{"error":"Failed to serialize events: {}"}}"#, e);
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            log_error!("Failed to serialize colony events: {}", e);
        }
    }
}


async fn handle_get_topology(stream: &mut tokio::net::TcpStream) {
    // Check colony status first
    let context = CoordinatorContext::get_instance();
    let status = {
        let stored_info = context.get_coord_stored_info();
        stored_info.status.clone()
    };
    
    // If status is Initializing, return in-progress response
    if matches!(status, ColonyStatus::Initializing) {
        let in_progress_json = r#"{"status":"in-progress"}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            in_progress_json.len(),
            in_progress_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
        return;
    }
    
    // Get topology instance (returns None if not initialized)
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            let error_json = r#"{"error":"Topology not initialized"}"#;
            let response = format!(
                "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return;
        }
    };
    
    // Get colony instance ID (clone it before dropping the guard)
    let instance_id = {
        let context = CoordinatorContext::get_instance();
        let stored_info = context.get_coord_stored_info();
        stored_info.colony_instance_id.clone()
    };
    
    // Create response with topology and instance_id
    #[derive(serde::Serialize)]
    struct TopologyResponse {
        #[serde(flatten)]
        topology: ClusterTopology,
        colony_instance_id: Option<String>,
    }
    
    let response_obj = TopologyResponse {
        topology: (*topology).clone(),
        colony_instance_id: instance_id,
    };
    
    match serde_json::to_string(&response_obj) {
        Ok(json) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            );
            if let Err(e) = stream.write_all(response.as_bytes()).await {
                log_error!("Failed to write topology response: {}", e);
            }
        }
        Err(e) => {
            let error_json = format!(r#"{{"error":"Failed to serialize topology: {}"}}"#, e);
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            log_error!("Failed to serialize topology: {}", e);
        }
    }
}

