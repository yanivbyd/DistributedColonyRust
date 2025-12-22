# Spec: Colony Instance ID System

**Status**: approved
**Created**: 2025-12-21

## Overview

Introduce a `colony_instance_id` (3-letter identifier) that serves multiple purposes:
1. Identify the logical colony instance (generated server-side during initialization)
2. Prefix for all colony artifacts (snapshots, stats)
3. **Future**: Enable multi-colony system by including colony ID in REST URLs (not part of this spec)

**Note**: This spec focuses on single-colony usage. Multi-colony support (URL routing by colony ID) might be addressed in a future spec.

**Architectural Context**: Currently, `/colony-start` initializes both the physical topology (cluster nodes, shard distribution) and the logical colony object (creatures, rules, state). In the future, these should be separated into distinct initialization phases to enable more flexible cluster management.

**Design Decision**: The client sends an `idempotency_key` (any string) for physical topology initialization. The server generates the 3-letter `colony_instance_id` for the logical colony. This separation supports the future split between topology and colony initialization.

## Current Implementation

- **Idempotency Key**: `crates/coordinator/src/coordinator_storage.rs:22` stores `colony_start_idempotency_key: Option<String>`
- **S3 Image Writes**: `crates/coordinator/src/colony_capture.rs:99` creates files as `YYYY_MM_DD__HH_MM_SS.png`
- **S3 Stats Writes**: `crates/coordinator/src/colony_stats.rs:210` creates files as `YYYY_MM_DD__HH_MM_SS.json`
- **GUI Cluster Tab**: `crates/gui/src/gui_main.rs:914-1025` displays 7-column grid with coordinator/backend info

## API Changes

### Modified Endpoints

#### `GET /topology`

**Before**:
```json
{
  "coordinator": {...},
  "shard_to_host": {...},
  "backend_to_shards": {...}
}
```

**After**:
```json
{
  "coordinator": {...},
  "shard_to_host": {...},
  "backend_to_shards": {...},
  "colony_instance_id": "abc"
}
```

**Changes**:
- Added `colony_instance_id` field (type: `Option<String>`)
- Field will be `null` if colony not yet initialized, otherwise contains 3-letter lowercase ID

### Behavioral Changes

#### S3 Snapshot Filenames

**Before**:
- Images: `output/s3/distributed-colony/images_shots/YYYY_MM_DD__HH_MM_SS.png`
- Stats: `output/s3/distributed-colony/stats_shots/YYYY_MM_DD__HH_MM_SS.json`

**After**:
- Images: `output/s3/distributed-colony/images_shots/{id}-YYYY_MM_DD__HH_MM_SS.png`
- Stats: `output/s3/distributed-colony/stats_shots/{id}-YYYY_MM_DD__HH_MM_SS.json`

Where `{id}` is the 3-letter colony instance ID (e.g., `abc-2025_12_21__14_30_45.png`)

## Affected Components

- Coordinator storage structure
- Colony start initialization
- HTTP endpoint validation
- Snapshot filename generation (images and stats)
- GUI cluster tab display
- GUI auto-initialization logic

## Implementation Plan

### 1. Shared Utilities (`crates/shared/src/utils.rs`)

Add helper function for server-side colony ID generation:

```rust
pub fn generate_colony_instance_id() -> String {
    let mut rng = new_random_generator();
    (0..3)
        .map(|_| (rng.gen_range(b'a'..=b'z') as char))
        .collect()
}
```

**Note**: This function is called by the coordinator during colony initialization, not by the client.

### 2. Coordinator Storage (`crates/coordinator/src/coordinator_storage.rs`)

Add new field for colony instance ID (line 22):

```rust
pub struct CoordinatorStoredInfo {
    pub colony_start_idempotency_key: Option<String>,  // For physical topology idempotency
    pub colony_instance_id: Option<String>,             // For logical colony identification (3 letters)
    // ... other fields
}
```

**Note**: We keep both fields - `idempotency_key` is for the physical topology initialization, `colony_instance_id` is for the logical colony.

### 3. Colony Start Logic (`crates/coordinator/src/colony_start.rs`)

Store idempotency key and generate colony instance ID (around line 70):

```rust
if let Some(key) = idempotency_key {
    let mut stored_info = context.get_coord_stored_info();
    stored_info.colony_start_idempotency_key = Some(key.clone());

    // Generate colony instance ID on server side
    let instance_id = shared::utils::generate_colony_instance_id();
    stored_info.colony_instance_id = Some(instance_id.clone());

    log!("Colony instance ID: {}", instance_id);
}
```

### 4. HTTP Server Validation (`crates/coordinator/src/http_server.rs`)

**Note**: The idempotency validation function remains unchanged - it validates against `colony_start_idempotency_key`, not `colony_instance_id`.

Update `/topology` endpoint to include colony instance ID (around line 446):

```rust
async fn handle_get_topology(stream: &mut tokio::net::TcpStream) {
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

    // Get colony instance ID
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    let instance_id = stored_info.colony_instance_id.clone();

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
```

### 5. Image Snapshot Filenames (`crates/coordinator/src/colony_capture.rs`)

Prefix filenames with instance ID (around line 99):

```rust
let stored_info = context.get_coord_stored_info();
let prefix = stored_info.colony_instance_id.as_deref()
    .expect("Colony instance ID must be set before capturing images");
let timestamp = format!("{}", now.format("%Y_%m_%d__%H_%M_%S"));
let filename = format!("{}-{}.png", prefix, timestamp);
let path = image_dir.join(filename);
```

### 6. Stats Snapshot Filenames (`crates/coordinator/src/colony_stats.rs`)

Prefix filenames with instance ID (around line 210):

```rust
let stored_info = context.get_coord_stored_info();
let prefix = stored_info.colony_instance_id.as_deref()
    .expect("Colony instance ID must be set before capturing stats");
let timestamp = format!("{}", now.format("%Y_%m_%d__%H_%M_%S"));
let filename = format!("{}-{}.json", prefix, timestamp);
let path = stats_dir.join(filename);
```

### 7. GUI Auto-Init (`crates/gui/src/gui_main.rs`)

**Note**: GUI auto-init remains unchanged. It continues to generate `gui-auto-{timestamp}` idempotency keys. The coordinator will generate the 3-letter colony instance ID server-side.

### 8. GUI AppState (`crates/gui/src/gui_main.rs`)

Add field to store instance ID (around line 61):

```rust
struct AppState {
    colony_instance_id: Option<String>,
    // ... existing fields
}
```

Initialize in `new()` method:

```rust
colony_instance_id: None,
```

### 9. GUI Fetch Instance ID (`crates/gui/src/gui_main.rs`)

Update existing topology fetch to extract `colony_instance_id` from the response. The topology response now includes a `colony_instance_id` field alongside the topology data.

Modify the topology deserialization structure or extraction logic to capture the `colony_instance_id` field and store it in `AppState.colony_instance_id`.

### 10. GUI Cluster Tab Display (`crates/gui/src/gui_main.rs`)

Add info section before the table (around line 920):

```rust
ui.heading("Cluster Topology");
ui.separator();

// Display instance ID in separate info section
if let Some(id) = &self.colony_instance_id {
    ui.horizontal(|ui| {
        ui.label("Colony Instance:");
        ui.label(egui::RichText::new(id).strong().color(egui::Color32::from_rgb(100, 200, 100)));
    });
    ui.separator();
}

// Existing table grid
egui::Grid::new("cluster_grid")
    .striped(true)
    // ... rest of table
```

## Files Changed

| File | Lines Changed | Description |
|------|---------------|-------------|
| `crates/shared/src/utils.rs` | +7 | Add `generate_colony_instance_id()` function (server-side) |
| `crates/coordinator/src/coordinator_storage.rs` | +1 | Add new field `colony_instance_id` (keep existing `colony_start_idempotency_key`) |
| `crates/coordinator/src/colony_start.rs` | +5 | Generate colony instance ID server-side during initialization |
| `crates/coordinator/src/http_server.rs` | ~40 | Extend `/topology` endpoint to include instance ID in response |
| `crates/coordinator/src/colony_capture.rs` | 3 | Prefix image filenames with instance ID |
| `crates/coordinator/src/colony_stats.rs` | 3 | Prefix stats filenames with instance ID |
| `crates/gui/src/gui_main.rs` | ~20 | Add field, extract from topology response, display in cluster tab |

**Total**: 7 files, ~80 lines of code

## Testing Strategy

### Unit Tests

Add to `crates/shared/tests/test_utils.rs`:

```rust
#[test]
fn test_generate_colony_instance_id() {
    let id = shared::utils::generate_colony_instance_id();
    assert_eq!(id.len(), 3);
    assert!(id.chars().all(|c| c.is_ascii_lowercase()));
}
```

**Note**: Existing integration tests in `crates/coordinator/tests/test_colony_start.rs` should pass without modification since they use valid idempotency keys.

---

**Status**: Ready for approval. Awaiting explicit instruction from Human Author to begin implementation.
