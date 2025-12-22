use shared::{log, log_error};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use shared::ssm;
use shared::be_api::{Shard, ColonyLifeRules, ShardLayer};
use crate::colony::Colony;
use crate::shard_utils::ShardUtils;
use crate::backend_config::{get_backend_hostname, get_backend_port};
use std::fmt::Write;
use std::sync::Mutex;
use std::time::Instant;

const HTTP_BIND_HOST: &str = "0.0.0.0";
const HTTP_LATENCY_WINDOW_SIZE: usize = 100;

#[derive(Debug, Clone)]
struct HttpLatencyStats {
    request_count: u32,
    total_latency_ms: f64,
    max_latency_ms: f64,
}

impl HttpLatencyStats {
    fn record(&mut self, latency_ms: f64) {
        self.request_count += 1;
        self.total_latency_ms += latency_ms;
        if latency_ms > self.max_latency_ms {
            self.max_latency_ms = latency_ms;
        }
    }

    fn reset(&mut self) {
        self.request_count = 0;
        self.total_latency_ms = 0.0;
        self.max_latency_ms = 0.0;
    }

    fn avg_latency_ms(&self) -> f64 {
        if self.request_count > 0 {
            self.total_latency_ms / self.request_count as f64
        } else {
            0.0
        }
    }
}

// Per-endpoint latency stats
static SHARD_IMAGE_STATS: Mutex<HttpLatencyStats> = Mutex::new(HttpLatencyStats {
    request_count: 0,
    total_latency_ms: 0.0,
    max_latency_ms: 0.0,
});

static SHARD_LAYER_STATS: Mutex<HttpLatencyStats> = Mutex::new(HttpLatencyStats {
    request_count: 0,
    total_latency_ms: 0.0,
    max_latency_ms: 0.0,
});

fn build_http_bind_addr(port: u16) -> String {
    format!("{}:{}", HTTP_BIND_HOST, port)
}

fn record_http_latency(endpoint: &str, latency_ms: f64, shard_id: &str) {
    // Determine which stats to update based on endpoint
    let is_image = endpoint.contains("/image");
    let stats = if is_image {
        &SHARD_IMAGE_STATS
    } else {
        &SHARD_LAYER_STATS
    };
    
    // Log slow requests
    if latency_ms > 500.0 {
        let backend_host = format!("{}:{}", get_backend_hostname(), get_backend_port());
        log!("Backend HTTP slow request: endpoint={}, latency_ms={:.2}, shard_id={}, host={}", 
             endpoint, latency_ms, shard_id, backend_host);
    } else if latency_ms > 200.0 {
        let backend_host = format!("{}:{}", get_backend_hostname(), get_backend_port());
        log!("Backend HTTP slow request: endpoint={}, latency_ms={:.2}, shard_id={}, host={}", 
             endpoint, latency_ms, shard_id, backend_host);
    }
    
    // Update stats
    let mut stats_guard = stats.lock().unwrap();
    stats_guard.record(latency_ms);
    
    // Log periodic aggregates (every 100 requests)
    if stats_guard.request_count >= HTTP_LATENCY_WINDOW_SIZE as u32 {
        let avg_latency = stats_guard.avg_latency_ms();
        let max_latency = stats_guard.max_latency_ms;
        let backend_host = format!("{}:{}", get_backend_hostname(), get_backend_port());
        
        // Get shard count
        let shard_count = if Colony::is_initialized() {
            let colony = Colony::instance();
            let (shards, _) = colony.get_hosted_shards();
            shards.len()
        } else {
            0
        };
        
        log!("Backend HTTP latency: endpoint={}, window_requests={}, avg_ms={:.2}, max_ms={:.2}, shards={}, host={}", 
             endpoint, stats_guard.request_count, avg_latency, max_latency, shard_count, backend_host);
        
        // Reset window
        stats_guard.reset();
    }
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
                        } else if request.starts_with("GET /api/shard/") {
                            // Parse shard endpoints: /api/shard/{shard_id}/image or /api/shard/{shard_id}/layer/{layer_name}
                            if request.find("/image").is_some() {
                                let shard_id = extract_shard_id(&request, "/api/shard/", "/image");
                                handle_get_shard_image(&mut stream, &shard_id).await;
                            } else if let Some(layer_start) = request.find("/layer/") {
                                let shard_id = extract_shard_id(&request, "/api/shard/", "/layer/");
                                let layer_name = extract_layer_name(&request, layer_start + "/layer/".len());
                                handle_get_shard_layer(&mut stream, &shard_id, &layer_name).await;
                            } else {
                                let error_json = r#"{"error":"Invalid shard endpoint"}"#;
                                let response = format!(
                                    "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                    error_json.len(),
                                    error_json
                                );
                                let _ = stream.write_all(response.as_bytes()).await;
                            }
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

fn extract_shard_id(request: &str, prefix: &str, suffix: &str) -> String {
    if let Some(start) = request.find(prefix) {
        let start_idx = start + prefix.len();
        if let Some(end) = request[start_idx..].find(suffix) {
            return request[start_idx..start_idx + end].to_string();
        }
    }
    String::new()
}

fn extract_layer_name(request: &str, start_idx: usize) -> String {
    // Extract layer name until space or newline (end of HTTP request line)
    let remaining = &request[start_idx..];
    if let Some(end) = remaining.find(|c: char| c == ' ' || c == '\r' || c == '\n') {
        remaining[..end].to_string()
    } else {
        remaining.to_string()
    }
}

fn layer_name_to_enum(layer_name: &str) -> Result<ShardLayer, String> {
    match layer_name {
        "creature-size" => Ok(ShardLayer::CreatureSize),
        "extra-food" => Ok(ShardLayer::ExtraFood),
        "can-kill" => Ok(ShardLayer::CanKill),
        "can-move" => Ok(ShardLayer::CanMove),
        "cost-per-turn" => Ok(ShardLayer::CostPerTurn),
        "food" => Ok(ShardLayer::Food),
        "health" => Ok(ShardLayer::Health),
        "age" => Ok(ShardLayer::Age),
        _ => Err(format!("Invalid layer name: {}", layer_name)),
    }
}

async fn handle_get_shard_image(stream: &mut tokio::net::TcpStream, shard_id: &str) {
    let start = Instant::now();
    let endpoint = "/api/shard/{id}/image";
    
    // Parse shard_id
    let shard = match Shard::from_id(shard_id) {
        Ok(s) => s,
        Err(e) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, e);
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let latency = start.elapsed();
            let latency_ms = latency.as_secs_f64() * 1000.0;
            record_http_latency(endpoint, latency_ms, shard_id);
            return;
        }
    };
    
    // Check if colony is initialized
    if !Colony::is_initialized() {
        let error_json = r#"{"error":"Colony not initialized"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            error_json.len(),
            error_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let latency = start.elapsed();
        let latency_ms = latency.as_secs_f64() * 1000.0;
        record_http_latency(endpoint, latency_ms, shard_id);
        return;
    }
    
    // Get shard image using existing handler logic
    let colony = Colony::instance();
    let rgb_bytes = if let Some(shard_arc) = colony.get_hosted_colony_shard_arc(&shard) {
        let image = {
            let shard_guard = shard_arc.lock().unwrap();
            ShardUtils::get_shard_image(&shard_guard, &shard)
        };
        if let Some(image) = image {
            // Convert Vec<Color> to raw RGB bytes (width * height * 3 bytes, row-major order)
            let width = shard.width as usize;
            let height = shard.height as usize;
            let mut rgb_bytes = Vec::with_capacity(width * height * 3);
            for color in &image {
                rgb_bytes.push(color.red);
                rgb_bytes.push(color.green);
                rgb_bytes.push(color.blue);
            }
            Some(rgb_bytes)
        } else {
            None
        }
    } else {
        None
    };
    
    if let Some(rgb_bytes) = rgb_bytes {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            rgb_bytes.len()
        );
        if let Err(e) = stream.write_all(response.as_bytes()).await {
            log_error!("Failed to write shard image response header: {}", e);
            return;
        }
        if let Err(e) = stream.write_all(&rgb_bytes).await {
            log_error!("Failed to write shard image response body: {}", e);
        }
    } else {
        let error_json = r#"{"error":"Shard not available"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            error_json.len(),
            error_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
    }
    
    // Record latency
    let latency = start.elapsed();
    let latency_ms = latency.as_secs_f64() * 1000.0;
    record_http_latency(endpoint, latency_ms, shard_id);
}

async fn handle_get_shard_layer(stream: &mut tokio::net::TcpStream, shard_id: &str, layer_name: &str) {
    let start = Instant::now();
    let endpoint = format!("/api/shard/{{id}}/layer/{}", layer_name);
    
    // Parse shard_id
    let shard = match Shard::from_id(shard_id) {
        Ok(s) => s,
        Err(e) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, e);
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let latency = start.elapsed();
            let latency_ms = latency.as_secs_f64() * 1000.0;
            record_http_latency(&endpoint, latency_ms, shard_id);
            return;
        }
    };
    
    // Parse layer name
    let layer = match layer_name_to_enum(layer_name) {
        Ok(l) => l,
        Err(e) => {
            let error_json = format!(r#"{{"error":"{}"}}"#, e);
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let latency = start.elapsed();
            let latency_ms = latency.as_secs_f64() * 1000.0;
            record_http_latency(&endpoint, latency_ms, shard_id);
            return;
        }
    };
    
    // Check if colony is initialized
    if !Colony::is_initialized() {
        let error_json = r#"{"error":"Colony not initialized"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            error_json.len(),
            error_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let latency = start.elapsed();
        let latency_ms = latency.as_secs_f64() * 1000.0;
        record_http_latency(&endpoint, latency_ms, shard_id);
        return;
    }
    
    // Get shard layer using existing handler logic
    let colony = Colony::instance();
    let binary_data = if let Some(shard_arc) = colony.get_hosted_colony_shard_arc(&shard) {
        let data = {
            let shard_guard = shard_arc.lock().unwrap();
            ShardUtils::get_shard_layer(&shard_guard, &shard, &layer)
        };
        if let Some(data) = data {
            // Convert to binary format: length (u32 LE) + i32 values (LE)
            let count = data.len() as u32;
            let mut binary_data = Vec::with_capacity(4 + data.len() * 4);
            binary_data.extend_from_slice(&count.to_le_bytes());
            for &value in &data {
                binary_data.extend_from_slice(&value.to_le_bytes());
            }
            Some(binary_data)
        } else {
            None
        }
    } else {
        None
    };
    
    if let Some(binary_data) = binary_data {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            binary_data.len()
        );
        if let Err(e) = stream.write_all(response.as_bytes()).await {
            log_error!("Failed to write shard layer response header: {}", e);
            return;
        }
        if let Err(e) = stream.write_all(&binary_data).await {
            log_error!("Failed to write shard layer response body: {}", e);
        }
    } else {
        let error_json = r#"{"error":"Shard not available"}"#;
        let response = format!(
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            error_json.len(),
            error_json
        );
        let _ = stream.write_all(response.as_bytes()).await;
    }
    
    // Record latency
    let latency = start.elapsed();
    let latency_ms = latency.as_secs_f64() * 1000.0;
    record_http_latency(&endpoint, latency_ms, shard_id);
}

