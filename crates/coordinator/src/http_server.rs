use shared::{log, log_error};
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::colony_start::colony_start_colony;
use crate::coordinator_context::CoordinatorContext;
use crate::coordinator_storage::ColonyStatus;
use crate::backend_client;
use shared::ssm;
use shared::cluster_topology::ClusterTopology;
use shared::coordinator_api::{ColonyEventDescription, ColonyMetricStats};
use shared::be_api::StatMetric;
use std::collections::{BTreeMap, HashMap};
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
                        } else if request.starts_with("POST /api/colony-stats") {
                            // Read full request body for POST requests
                            let body = read_post_body(&mut stream, &request, &buffer[..n]).await;
                            handle_post_colony_stats(&mut stream, &body).await;
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

async fn read_post_body(stream: &mut tokio::net::TcpStream, request: &str, initial_buffer: &[u8]) -> String {
    // Try to extract Content-Length from headers
    let content_length = request
        .lines()
        .find(|line| line.to_lowercase().starts_with("content-length:"))
        .and_then(|line| {
            line.split(':')
                .nth(1)
                .and_then(|s| s.trim().parse::<usize>().ok())
        });
    
    if let Some(len) = content_length {
        // Find where body starts (after \r\n\r\n)
        let body_start = request.find("\r\n\r\n")
            .or_else(|| request.find("\n\n"))
            .map(|pos| pos + 4)
            .unwrap_or(request.len());
        
        let body_in_request = if body_start < initial_buffer.len() {
            String::from_utf8_lossy(&initial_buffer[body_start..]).to_string()
        } else {
            String::new()
        };
        
        // If we need more bytes, read them
        if body_in_request.len() < len {
            let mut remaining = vec![0u8; len - body_in_request.len()];
            if let Ok(_) = stream.read_exact(&mut remaining).await {
                body_in_request + &String::from_utf8_lossy(&remaining)
            } else {
                body_in_request
            }
        } else {
            body_in_request[..len.min(body_in_request.len())].to_string()
        }
    } else {
        // No Content-Length, try to read from initial buffer
        let body_start = request.find("\r\n\r\n")
            .or_else(|| request.find("\n\n"))
            .map(|pos| pos + 4)
            .unwrap_or(request.len());
        
        if body_start < initial_buffer.len() {
            String::from_utf8_lossy(&initial_buffer[body_start..]).to_string()
        } else {
            String::new()
        }
    }
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

async fn handle_post_colony_stats(stream: &mut tokio::net::TcpStream, body: &str) {
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
    
    // Parse request body
    #[derive(serde::Deserialize)]
    struct Request {
        metrics: Vec<String>,
    }
    
    let request: Request = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => {
            let error_json = format!(r#"{{"error":"Invalid request body: {}"}}"#, e);
            let response = format!(
                "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            return;
        }
    };
    
    // Convert string metrics to StatMetric enum
    let mut stat_metrics = Vec::new();
    for metric_str in &request.metrics {
        let metric = match metric_str.as_str() {
            "Health" => StatMetric::Health,
            "CreatureSize" => StatMetric::CreatureSize,
            "CreateCanKill" => StatMetric::CreateCanKill,
            "CreateCanMove" => StatMetric::CreateCanMove,
            "Food" => StatMetric::Food,
            "Age" => StatMetric::Age,
            _ => {
                let error_json = format!(r#"{{"error":"Invalid metric: {}"}}"#, metric_str);
                let response = format!(
                    "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    error_json.len(),
                    error_json
                );
                let _ = stream.write_all(response.as_bytes()).await;
                return;
            }
        };
        stat_metrics.push(metric);
    }
    
    // Call handler logic (duplicated from coordinator_main since it's not accessible as a module)
    let (tick_count, metrics) = handle_get_colony_stats_http(stat_metrics).await;
    
    // Build JSON response
    #[derive(serde::Serialize)]
    struct MetricResponse {
        metric: String,
        avg: f64,
        buckets: Vec<shared::be_api::StatBucket>,
    }
    
    #[derive(serde::Serialize)]
    struct Response {
        tick_count: u64,
        metrics: Vec<MetricResponse>,
    }
    
    let metric_responses: Vec<MetricResponse> = metrics.into_iter().map(|m| {
        let metric_str = match m.metric {
            StatMetric::Health => "Health",
            StatMetric::CreatureSize => "CreatureSize",
            StatMetric::CreateCanKill => "CreateCanKill",
            StatMetric::CreateCanMove => "CreateCanMove",
            StatMetric::Food => "Food",
            StatMetric::Age => "Age",
        };
        MetricResponse {
            metric: metric_str.to_string(),
            avg: m.avg,
            buckets: m.buckets,
        }
    }).collect();
    
    match serde_json::to_string(&Response { tick_count, metrics: metric_responses }) {
        Ok(json) => {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                json.len(),
                json
            );
            if let Err(e) = stream.write_all(response.as_bytes()).await {
                log_error!("Failed to write colony-stats response: {}", e);
            }
        }
        Err(e) => {
            let error_json = format!(r#"{{"error":"Failed to serialize stats: {}"}}"#, e);
            let response = format!(
                "HTTP/1.1 500 Internal Server Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                error_json.len(),
                error_json
            );
            let _ = stream.write_all(response.as_bytes()).await;
            log_error!("Failed to serialize colony stats: {}", e);
        }
    }
}

async fn handle_get_colony_stats_http(metrics: Vec<StatMetric>) -> (u64, Vec<ColonyMetricStats>) {
    // Aggregate across all shards
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Topology not initialized");
            return (0, Vec::new());
        }
    };
    let shards = topology.get_all_shards();
    if shards.is_empty() {
        return (0, Vec::new());
    }

    // Prepare index mapping for requested metrics
    fn metric_id(m: StatMetric) -> u8 {
        match m {
            StatMetric::Health => 0,
            StatMetric::CreatureSize => 1,
            StatMetric::CreateCanKill => 2,
            StatMetric::CreateCanMove => 3,
            StatMetric::Food => 4,
            StatMetric::Age => 5,
        }
    }
    let mut pos_by_id: HashMap<u8, usize> = HashMap::new();
    for (idx, m) in metrics.iter().copied().enumerate() {
        pos_by_id.insert(metric_id(m), idx);
    }
    // counts_per_metric: per requested metric (by index) -> value -> occs
    let mut counts_per_metric: Vec<BTreeMap<i32, u64>> = vec![BTreeMap::new(); metrics.len()];

    let mut min_tick: Option<u64> = None;
    for shard in shards {
        if let Some((tick, per_metric)) = backend_client::call_backend_get_shard_stats(shard, metrics.clone()) {
            min_tick = Some(match min_tick { Some(t) => t.min(tick), None => tick });
            for (metric, buckets) in per_metric {
                if let Some(&idx) = pos_by_id.get(&metric_id(metric)) {
                    let entry = counts_per_metric.get_mut(idx).unwrap();
                    for b in buckets {
                        *entry.entry(b.value).or_insert(0) += b.occs;
                    }
                }
            }
        }
    }

    // Build ordered results following the requested metrics order
    let mut results: Vec<ColonyMetricStats> = Vec::with_capacity(metrics.len());
    for (i, metric) in metrics.into_iter().enumerate() {
        let counts = std::mem::take(&mut counts_per_metric[i]);
        let mut sum: i64 = 0;
        let mut total: i64 = 0;
        for (value, occs) in &counts {
            sum += *value as i64 * *occs as i64;
            total += *occs as i64;
        }
        let avg = if total > 0 { sum as f64 / total as f64 } else { 0.0 };
        let buckets = counts.into_iter().map(|(value, occs)| shared::be_api::StatBucket { value, occs }).collect();
        results.push(ColonyMetricStats { metric, avg, buckets });
    }

    (min_tick.unwrap_or(0), results)
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

