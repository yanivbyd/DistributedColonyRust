use std::net::TcpListener;
use std::io::{Read, Write};
use lazy_static::lazy_static;
use std::time::Instant;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    static ref METRICS: Mutex<HashMap<&'static str, (f64, u64)>> = Mutex::new(HashMap::new());
}

pub fn publish_latency(metric_name: &'static str, value: f64) {
    let mut metrics = METRICS.lock().unwrap();
    let entry = metrics.entry(metric_name).or_insert((0.0, 0));
    entry.0 += value;
    entry.1 += 1;
}

pub fn start_metrics_endpoint() {
    std::thread::spawn(|| {
        println!("[metrics] Metrics endpoint thread started");
        let listener = TcpListener::bind("0.0.0.0:9898").expect("Failed to bind metrics endpoint");
        for stream in listener.incoming() {
            match stream {
                Ok(mut stream) => {
                    // Read the incoming HTTP request data before responding
                    let mut request_buffer = [0; 1024];
                    if let Err(e) = stream.read(&mut request_buffer) {
                        eprintln!("[metrics] Failed to read request: {}", e);
                        continue;
                    }

                    // Gather averages
                    let metrics = METRICS.lock().unwrap();
                    let mut response = String::new();
                    for (name, (sum, count)) in metrics.iter() {
                        let avg = if *count > 0 {
                            format!("{:.3}", sum / (*count as f64))
                        } else {
                            "N/A".to_string()
                        };
                        response.push_str(&format!("{}: {}\n", name, avg));
                    }

                    // Write HTTP response header
                    let header = b"HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n";
                    if let Err(e) = stream.write_all(header) {
                        eprintln!("[metrics] Failed to write header: {}", e);
                        continue;
                    }

                    // Write metrics body
                    if let Err(e) = stream.write_all(response.as_bytes()) {
                        eprintln!("[metrics] Failed to write metrics: {}", e);
                        continue;
                    }
                }
                Err(e) => {
                    eprintln!("[metrics] Connection failed: {}", e);
                }
            }
        }
    });
}

pub struct LatencyMonitor {
    start: Instant,
    metric_name: &'static str,
}

impl LatencyMonitor {
    pub fn start(metric_name: &'static str) -> Self {
        LatencyMonitor { start: Instant::now(), metric_name }
    }
}

impl Drop for LatencyMonitor {
    fn drop(&mut self) {
        let millis = self.start.elapsed().as_millis() as f64;
        publish_latency(self.metric_name, millis);
    }
}
