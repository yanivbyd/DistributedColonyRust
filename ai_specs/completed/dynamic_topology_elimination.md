# Dynamic Topology Elimination

## Spec Header

**Status**: approved

---

## Overview

This specification defines a design to eliminate static topology configuration from the codebase. Currently, topology is defined through hardcoded constants (ports, hostnames, shard dimensions) that are initialized lazily on first access. The goal is to replace this with a fully dynamic topology system.

**Current Implementation State**:
- Backends retrieve topology from coordinator during `InitColonyShard` (completed - see `backend_topology_retrieval_from_coordinator.md`)
- GUI retrieves topology via HTTP API from coordinator (completed - `GET /topology` endpoint)
- Static topology constants and methods removed (completed)
- Explicit initialization implemented (completed)
- Instance methods for dimensions added (completed)
- GUI automatically initiates colony-start on 404 (completed)
- Coordinator never initializes topology automatically (completed)
- Backend discovery works in both localhost and AWS modes (completed)

**Remaining Work**:
- ✅ All work completed - spec ready for review

## Architecture

### Topology Flow

1. **Coordinator Startup**: Coordinator starts without topology. **Topology is never created automatically** - it must be created explicitly via `POST /colony-start`:
   - Discovers available backend nodes via `ClusterRegistry` (node addresses and ports only)
   - Uses default shard dimensions for first initialization
   - Creates shard grid and distributes shards evenly across discovered backends
   - Stores cluster configuration (shard dimensions) in coordinator context (in memory only, never persisted to disk)
   - Initializes `ClusterTopology` in memory (topology contains shard dimensions via shard map)
   - `GET /topology` returns 404 if topology not initialized

2. **Backend Startup**: Backends discover coordinator via `ClusterRegistry` and retrieve topology during `InitColonyShard` (already implemented).

3. **GUI Startup**: GUI discovers coordinator via `ClusterRegistry` and attempts to retrieve topology via `GET /topology` HTTP endpoint:
   - If topology exists: GUI receives topology and proceeds normally
   - If topology not initialized (404 response): GUI automatically initiates `POST /colony-start` to create topology, then retries `GET /topology` with exponential backoff

### Configuration Storage

**ClusterRegistry Scope**:
- `ClusterRegistry` is only for node discovery (addresses and ports)
- Does NOT store cluster configuration (shard dimensions)

**Cluster Configuration Storage**:
- Shard dimensions are stored in coordinator context (in memory only, never persisted to disk)
- Dimensions can be derived from topology's shard map (shards contain width/height)
- For first initialization, coordinator uses default values
- Other processes (GUI, backends) get dimensions from topology via HTTP API or RPC

## Implementation Changes (Completed)

### 1. Remove Static Topology Constants

**Delete from `cluster_topology.rs`**:
- `COORDINATOR_PORT`, `BACKEND_PORTS`, `HOSTNAME`
- `WIDTH_IN_SHARDS`, `HEIGHT_IN_SHARDS`, `SHARD_WIDTH`, `SHARD_HEIGHT`
- `new_fixed_topology()` method
- Static methods: `get_backend_ports()`, `get_coordinator_port()`, `get_hostname()`
- Static dimension methods: `get_width_in_shards()`, `get_height_in_shards()`, `get_shard_width()`, `get_shard_height()`

### 2. Implement Explicit Initialization

**Replace lazy initialization**:
- Change `get_instance()` to return `Option<Arc<ClusterTopology>>` (returns `None` if not initialized)
- Add `initialize(config: TopologyConfig) -> Result<Arc<ClusterTopology>, TopologyError>`
- Require explicit initialization before use (fail-fast if not initialized)
- Remove `initialize_with_dynamic_topology()` in favor of unified `initialize()`

### 3. Add Instance Methods for Dimensions

**Replace static dimension access**:
- Add instance methods: `width_in_shards()`, `height_in_shards()`, `shard_width()`, `shard_height()`
- Update all call sites to use instance methods instead of static methods

### 4. Update Coordinator Colony-Start

**Modify `colony_start.rs`** (renamed from `cloud_start.rs`):
- Use default shard dimensions for first initialization
- Store cluster configuration (shard dimensions) in coordinator context (in memory) after creating topology
- Use instance methods for dimensions instead of static methods
- Ensure no automatic topology initialization occurs at coordinator startup
- Use `ClusterRegistry` for backend discovery (works in both localhost and AWS modes)
- Compare both IP and port when excluding coordinator from backend list (critical for localhost mode)

### 5. Update GUI to Handle Missing Topology

**Modify `gui_main.rs`**:
- When `GET /topology` returns 404, automatically call `POST /colony-start` (with generated idempotency key)
- After colony-start completes, retry `GET /topology` with exponential backoff (up to 10 retries, ~10 seconds total)
- Handle colony-start errors gracefully (show error to user)

## Impact Analysis

**High Impact Files**:
- `crates/shared/src/cluster_topology.rs`: Remove static constants/methods, add explicit initialization
- `crates/coordinator/src/colony_start.rs`: Use default dimensions or coordinator context, instance methods for dimensions, use ClusterRegistry for backend discovery (works in both localhost and AWS modes)

**Medium Impact Files**:
- `crates/coordinator/src/init_colony.rs`: Replace static dimension calls with instance methods
- `crates/coordinator/src/global_topography.rs`: Replace static dimension calls
- `crates/gui/src/gui_main.rs`: Replace static dimension calls, add colony-start initiation on 404
- `crates/coordinator/src/coordinator_main.rs`: Ensure no automatic topology initialization
- `crates/coordinator/src/coordinator_storage.rs`: Rename `cloud_start_idempotency_key` to `colony_start_idempotency_key`

## Success Criteria

✅ No hardcoded topology constants in `cluster_topology.rs`
✅ Cluster configuration stored in coordinator context (in memory)
✅ Explicit initialization required (no lazy initialization fallback)
✅ All dimension access via instance methods
✅ Coordinator never initializes topology automatically (always requires explicit colony-start)
✅ GUI automatically initiates colony-start when topology is missing (404 response)
✅ Works in both localhost and AWS deployment modes
✅ Clear error messages when topology cannot be initialized
✅ Backend discovery works correctly in localhost mode (compares IP+port, not just IP)

## Implementation Notes

### Backend Discovery in Localhost Mode

In localhost mode, the coordinator and all backends share the same IP address (`127.0.0.1`). The backend discovery logic in `colony_start.rs` must compare both IP address and port when excluding the coordinator from the backend list. Comparing only the IP address would incorrectly filter out all backends.

**Correct implementation**: Compare both IP and port:
```rust
if backend_address.ip == coordinator_address.ip && 
   backend_address.internal_port == coordinator_internal_port {
    // Skip coordinator
}
```

**Incorrect implementation** (would skip all backends in localhost):
```rust
if backend_address.ip == coordinator_ip {
    // This would skip all backends in localhost mode!
}
```

### CoordinatorContext Initialization

The `CoordinatorContext` is initialized lazily when `get_instance()` is first called. This can happen early in the coordinator's lifecycle (e.g., when the HTTP server checks if the colony is already started). 

When `initialize_colony()` is called during colony-start, it must not attempt to re-initialize the context using `initialize_with_stored_info()`, as this will panic if the context is already initialized. Instead, it should:

1. Get the existing context instance via `get_instance()`
2. Reset the stored info by directly updating the mutex-protected data

**Correct implementation**:
```rust
let context = CoordinatorContext::get_instance();
// Reset stored info to fresh state (context is already initialized, so we just update the data)
{
    let mut stored_info = context.get_coord_stored_info();
    *stored_info = CoordinatorStoredInfo::new();
}
```

**Incorrect implementation** (would panic if context already initialized):
```rust
CoordinatorContext::initialize_with_stored_info(CoordinatorStoredInfo::new());
```

### Unit Tests

Unit tests have been added in `crates/coordinator/tests/test_colony_start.rs` to verify the backend filtering logic:
- `test_filter_backends_excluding_coordinator_localhost_mode`: Verifies that in localhost mode (same IP), backends with different ports are not incorrectly filtered out
- `test_filter_backends_excluding_coordinator_different_ips`: Verifies that in AWS mode (different IPs), the filtering works correctly
- `test_filter_backends_excluding_coordinator_same_ip_different_port`: Specifically tests the critical case where IPs match but ports differ

These tests use a helper function that isolates the IP+port comparison logic from the backend status check, making the tests deterministic and fast.
