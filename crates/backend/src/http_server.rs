use shared::{log, log_error};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use shared::ssm;
use shared::be_api::{Shard, ColonyLifeRules};
use crate::colony::Colony;
use std::fmt::Write;

const HTTP_BIND_HOST: &str = "0.0.0.0";

fn build_http_bind_addr(port: u16) -> String {
    format!("{}:{}", HTTP_BIND_HOST, port)
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
                        
                        if request.starts_with("GET /api/colony-info") {
                            handle_get_colony_info(&mut stream).await;
                        } else if request.starts_with("GET /debug-ssm") {
                            let body = render_ssm_state().await;
                            let response = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
                                body.len(),
                                body
                            );
                            let _ = stream.write_all(response.as_bytes()).await;
                        } else if request.starts_with("GET /") {
                            let response = "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 13\r\n\r\nBackend API";
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

async fn handle_get_colony_info(stream: &mut tokio::net::TcpStream) {
    // Check if colony is initialized
    if !Colony::is_initialized() {
        let error_json = r#"{"error":"Colony not initialized"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            error_json.len(),
            error_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
        return;
    }
    
    // Get colony info using existing handler logic
    let colony = Colony::instance();
    let (shards, shard_arcs) = colony.get_hosted_shards();
    
    // Get ColonyLifeRules and current_tick from the first available shard
    let (colony_life_rules, current_tick) = if let Some(first_shard_arc) = shard_arcs.first() {
        let shard = first_shard_arc.lock().unwrap();
        (Some(shard.colony_life_rules), Some(shard.current_tick))
    } else {
        (None, None)
    };
    
    // Build JSON response
    #[derive(serde::Serialize)]
    struct Response {
        width: i32,
        height: i32,
        shards: Vec<Shard>,
        colony_life_rules: Option<ColonyLifeRules>,
        current_tick: Option<u64>,
    }
    
    let response_data = Response {
        width: colony._width,
        height: colony._height,
        shards,
        colony_life_rules,
        current_tick,
    };
    
    match serde_json::to_string(&response_data) {
        Ok(json) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            );
            if let Err(e) = stream.write_all(response.as_bytes()).await {
                log_error!("Failed to write colony-info response: {}", e);
            }
        }
        Err(e) => {
            let error_json = format!(r#"{{"error":"Failed to serialize colony info: {}"}}"#, e);
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            log_error!("Failed to serialize colony info: {}", e);
        }
    }
}

