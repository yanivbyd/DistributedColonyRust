use shared::cluster_topology::HostInfo;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationType {
    GetShardImage,
    GetShardLayer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationKey {
    pub operation_type: OperationType,
    pub target_node: HostInfo,
}

impl OperationKey {
    pub fn new(operation_type: OperationType, target_node: HostInfo) -> Self {
        Self {
            operation_type,
            target_node,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub avg_ms: f64,
    pub sample_count: usize,
    pub error_count: usize,
}

#[derive(Debug, Clone)]
pub struct NodeStats {
    pub image_latency: Option<LatencyStats>,
    pub layer_latency: Option<LatencyStats>,
    pub total_error_rate: f64,
}

pub struct LatencyTracker {
    measurements: Arc<Mutex<HashMap<OperationKey, VecDeque<Duration>>>>,
    errors: Arc<Mutex<HashMap<OperationKey, usize>>>,
    max_samples: usize,
}

impl LatencyTracker {
    pub fn new(max_samples: usize) -> Self {
        Self {
            measurements: Arc::new(Mutex::new(HashMap::new())),
            errors: Arc::new(Mutex::new(HashMap::new())),
            max_samples,
        }
    }

    pub fn record_success(&self, key: OperationKey, latency: Duration) {
        let mut measurements = self.measurements.lock().expect("Failed to lock measurements");
        let samples = measurements.entry(key).or_insert_with(VecDeque::new);

        samples.push_back(latency);

        if samples.len() > self.max_samples {
            samples.pop_front();
        }
    }

    pub fn record_error(&self, key: OperationKey) {
        let mut errors = self.errors.lock().expect("Failed to lock errors");
        *errors.entry(key).or_insert(0) += 1;
    }

    pub fn get_stats(&self, key: &OperationKey) -> Option<LatencyStats> {
        let measurements = self.measurements.lock().expect("Failed to lock measurements");
        let errors = self.errors.lock().expect("Failed to lock errors");

        let samples = measurements.get(key)?;
        if samples.is_empty() {
            return None;
        }

        let total_ms: f64 = samples.iter().map(|d| d.as_secs_f64() * 1000.0).sum();
        let avg_ms = total_ms / samples.len() as f64;

        let error_count = errors.get(key).copied().unwrap_or(0);

        Some(LatencyStats {
            avg_ms,
            sample_count: samples.len(),
            error_count,
        })
    }

    pub fn get_node_stats(&self, node: &HostInfo) -> NodeStats {
        let image_key = OperationKey::new(OperationType::GetShardImage, node.clone());
        let layer_key = OperationKey::new(OperationType::GetShardLayer, node.clone());

        let image_latency = self.get_stats(&image_key);
        let layer_latency = self.get_stats(&layer_key);

        let total_samples = image_latency.as_ref().map(|s| s.sample_count).unwrap_or(0)
            + layer_latency.as_ref().map(|s| s.sample_count).unwrap_or(0);

        let total_errors = image_latency.as_ref().map(|s| s.error_count).unwrap_or(0)
            + layer_latency.as_ref().map(|s| s.error_count).unwrap_or(0);

        let total_requests = total_samples + total_errors;

        let total_error_rate = if total_requests > 0 {
            (total_errors as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        };

        NodeStats {
            image_latency,
            layer_latency,
            total_error_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_host(port: u16) -> HostInfo {
        HostInfo {
            hostname: "127.0.0.1".to_string(),
            port,
        }
    }

    #[test]
    fn test_record_success_adds_samples() {
        let tracker = LatencyTracker::new(100);
        let host = create_test_host(8080);
        let key = OperationKey::new(OperationType::GetShardImage, host);

        tracker.record_success(key.clone(), Duration::from_millis(10));
        tracker.record_success(key.clone(), Duration::from_millis(20));

        let stats = tracker.get_stats(&key).expect("Stats should exist");
        assert_eq!(stats.sample_count, 2);
        assert_eq!(stats.avg_ms, 15.0);
    }

    #[test]
    fn test_rolling_window_maintains_max_samples() {
        let tracker = LatencyTracker::new(3);
        let host = create_test_host(8080);
        let key = OperationKey::new(OperationType::GetShardImage, host);

        tracker.record_success(key.clone(), Duration::from_millis(10));
        tracker.record_success(key.clone(), Duration::from_millis(20));
        tracker.record_success(key.clone(), Duration::from_millis(30));
        tracker.record_success(key.clone(), Duration::from_millis(40));

        let stats = tracker.get_stats(&key).expect("Stats should exist");
        assert_eq!(stats.sample_count, 3);
        assert_eq!(stats.avg_ms, 30.0);
    }

    #[test]
    fn test_error_counting() {
        let tracker = LatencyTracker::new(100);
        let host = create_test_host(8080);
        let key = OperationKey::new(OperationType::GetShardImage, host);

        tracker.record_success(key.clone(), Duration::from_millis(10));
        tracker.record_error(key.clone());
        tracker.record_error(key.clone());

        let stats = tracker.get_stats(&key).expect("Stats should exist");
        assert_eq!(stats.sample_count, 1);
        assert_eq!(stats.error_count, 2);
    }

    #[test]
    fn test_node_stats_aggregates_both_operations() {
        let tracker = LatencyTracker::new(100);
        let host = create_test_host(8080);

        let image_key = OperationKey::new(OperationType::GetShardImage, host.clone());
        let layer_key = OperationKey::new(OperationType::GetShardLayer, host.clone());

        tracker.record_success(image_key.clone(), Duration::from_millis(10));
        tracker.record_success(image_key.clone(), Duration::from_millis(20));
        tracker.record_error(image_key);

        tracker.record_success(layer_key.clone(), Duration::from_millis(5));
        tracker.record_error(layer_key.clone());
        tracker.record_error(layer_key);

        let node_stats = tracker.get_node_stats(&host);

        assert!(node_stats.image_latency.is_some());
        assert!(node_stats.layer_latency.is_some());

        let total_samples = 2 + 1;
        let total_errors = 1 + 2;
        let expected_error_rate = (total_errors as f64 / (total_samples + total_errors) as f64) * 100.0;

        assert_eq!(node_stats.total_error_rate, expected_error_rate);
    }

    #[test]
    fn test_no_samples_returns_none() {
        let tracker = LatencyTracker::new(100);
        let host = create_test_host(8080);
        let key = OperationKey::new(OperationType::GetShardImage, host);

        assert!(tracker.get_stats(&key).is_none());
    }

    #[test]
    fn test_error_rate_calculation() {
        let tracker = LatencyTracker::new(100);
        let host = create_test_host(8080);

        let image_key = OperationKey::new(OperationType::GetShardImage, host.clone());

        tracker.record_success(image_key.clone(), Duration::from_millis(10));
        tracker.record_success(image_key.clone(), Duration::from_millis(20));
        tracker.record_error(image_key.clone());
        tracker.record_error(image_key);

        let node_stats = tracker.get_node_stats(&host);

        let expected_error_rate = (2.0 / 4.0) * 100.0;
        assert_eq!(node_stats.total_error_rate, expected_error_rate);
    }
}
