# Specification: GUI Cluster Tab - Latency Information Display

## Spec Header

### Status - approved

---

## Main Specification

### Overview

Extend the GUI Cluster tab to display network latency information for HTTP operations performed by the GUI. This will help users diagnose performance issues and identify slow nodes in the distributed colony cluster.

### Goals

1. Track latency for shard image and layer data operations per node
2. Display average latency in two new columns in the Cluster tab grid
3. Display error rate percentage for each node (aggregated across both operations)

### Non-Goals

- Tracking latency for coordinator operations (stats, events, topology)
- Percentile calculations (p50, p95, p99)
- Historical graphs or time-series visualization

### Architecture

#### Component: LatencyTracker

**Purpose**: Collect and aggregate latency measurements for HTTP operations

**Location**: `crates/gui/src/latency_tracker.rs` (new file)

**Data Structures**:

```rust
pub struct LatencyTracker {
    measurements: Arc<Mutex<HashMap<OperationKey, VecDeque<Duration>>>>,
    errors: Arc<Mutex<HashMap<OperationKey, usize>>>,
    max_samples: usize,  // Default: 100
}

pub struct OperationKey {
    operation_type: OperationType,
    target_node: HostInfo,
}

pub enum OperationType {
    GetShardImage,   // /api/shard/{id}/image
    GetShardLayer,   // /api/shard/{id}/layer/{layer}
}

pub struct LatencyStats {
    pub avg_ms: f64,
    pub sample_count: usize,
    pub error_count: usize,
}
```

**Methods**:

```rust
impl LatencyTracker {
    pub fn new(max_samples: usize) -> Self;

    pub fn record_success(&self, key: OperationKey, latency: Duration);

    pub fn record_error(&self, key: OperationKey);

    pub fn get_stats(&self, key: &OperationKey) -> Option<LatencyStats>;

    pub fn get_node_stats(&self, node: &HostInfo) -> NodeStats;
}

pub struct NodeStats {
    pub image_latency: Option<LatencyStats>,
    pub layer_latency: Option<LatencyStats>,
    pub total_error_rate: f64,  // Aggregated across both operations
}
```

#### Component: Instrumented HTTP Client

**Modifications to**: `crates/gui/src/gui_main.rs`

**Changes**:
1. Add `latency_tracker: Arc<LatencyTracker>` field to `ColonyGuiApp` struct
2. Wrap all HTTP calls with latency measurement:
   ```rust
   let start = Instant::now();
   let result = http_client.get(url).send();
   let latency = start.elapsed();

   match result {
       Ok(_) => latency_tracker.record_success(key, latency),
       Err(_) => latency_tracker.record_error(key),
   }
   ```

**Affected Functions**:
- `get_all_shard_retained_images()` - wraps `/api/shard/{id}/image` (GetShardImage)
- `get_all_shard_layer_data()` - wraps `/api/shard/{id}/layer/{layer}` (GetShardLayer)
- `get_shard_color_data()` - wraps `/api/shard/{id}/image` (GetShardImage)

#### Component: Cluster Tab UI Extensions

**Modifications to**: `show_cluster_tab()` method in `gui_main.rs`

**New UI Layout** - Extended grid with three new columns:

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│ Cluster Topology (localhost mode)                                                      │
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ Role       │ Hostname  │ RPC Port │ HTTP Port │ Shards │ Img Lat │ Layer Lat │ Err %  │
│ Coordinator│ 127.0.0.1 │ 8082     │ 8083      │ -      │ N/A     │ N/A       │ N/A    │
│ Backend    │ 127.0.0.1 │ 8084     │ 8085      │ 16     │ 12ms    │ 8ms       │ 0.5%   │
│ Backend    │ 127.0.0.1 │ 8086     │ 8087      │ 16     │ 15ms    │ 10ms      │ 2.1%   │
└─────────────────────────────────────────────────────────────────────────────────────────┘

Legend:
- Img Lat: Average latency for shard image requests (last 100 samples)
- Layer Lat: Average latency for layer data requests (last 100 samples)
- Err %: Error rate (failed requests / total requests * 100) across both operations
- N/A: No samples collected yet or node doesn't serve these operations
```

**Implementation Details**:
1. Add three new columns to the egui Grid: "Img Lat", "Layer Lat", and "Err %"
2. For each backend, calculate stats from `latency_tracker.get_stats()`
3. Error rate calculation: `(error_count / (error_count + sample_count)) * 100`
4. Display "N/A" for coordinator (doesn't serve shard operations)
5. Display "N/A" if no requests attempted yet (both success and error count are 0)
6. Format: "12ms" for latency, "0.5%" for error rate (one decimal place)

### Data Flow

```
┌─────────────┐
│ GUI HTTP    │
│ Request     │
└──────┬──────┘
       │
       │ (start timer)
       v
┌─────────────┐
│ HTTP Call   │───┐
│ (reqwest)   │   │ (success/error)
└──────┬──────┘   │
       │          │
       │          v
       │   ┌─────────────────┐
       │   │ LatencyTracker  │
       │   │ .record_xxx()   │
       │   └─────────────────┘
       v
┌─────────────┐
│ Update      │
│ GUI State   │
└─────────────┘
       │
       v
┌─────────────┐
│ Cluster Tab │
│ Display     │◄────────┐
└─────────────┘         │
                        │
                 ┌──────┴──────┐
                 │ Get Stats   │
                 │ on Render   │
                 └─────────────┘
```

### Implementation Steps

1. Create `crates/gui/src/latency_tracker.rs` with `LatencyTracker` struct and methods
2. Add `latency_tracker: Arc<LatencyTracker>` field to `ColonyGuiApp` struct
3. Instrument HTTP calls in:
   - `get_all_shard_retained_images()`
   - `get_all_shard_layer_data()`
   - `get_shard_color_data()`
4. Extend `show_cluster_tab()` to add three new columns: "Img Lat", "Layer Lat", "Err %"
5. Add unit tests for `LatencyTracker` (rolling window, average calculation, error rate)
6. Test in localhost and AWS modes

### Edge Cases

1. **No samples yet**: Display "N/A" when `sample_count == 0`
2. **All requests failing**: Display "N/A" with error count tracked separately
3. **Node becomes unreachable**: Existing stats remain visible, new requests record errors
4. **Rolling window overflow**: Drop oldest sample when VecDeque exceeds 100 entries

### Testing

**Unit Tests**:
- Rolling window maintains max 100 samples per operation
- Average calculation is correct
- Error counting works independently of latency samples

**Manual Testing**:
- Verify latency display in localhost mode (~5-15ms typical)
- Verify latency display in AWS mode (~50-500ms depending on region)
- Verify "N/A" shows when no samples collected

