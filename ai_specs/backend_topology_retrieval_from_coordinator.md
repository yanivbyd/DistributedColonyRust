# Backend Topology Retrieval from Coordinator

## Acknowledgment
Status: Pending review

## Overview
This specification defines the first subtask toward achieving "Dynamic Topology Elimination". The goal is to make the coordinator the single source of truth for topology information by ensuring backends retrieve topology from the coordinator rather than using static definitions.

This phase does not change how topology is created:
- **Localhost mode**: Topology creation remains static (using constants in `cluster_topology.rs`)
- **AWS mode**: Topology is created in `cloud_start` (as it currently is)
- **Coordinator**: Continues to create and maintain topology as it does now
- **GUI**: Continues to use static topology (unchanged in this phase)

**What this phase changes**:
- **Backend topology retrieval**: Backends will no longer use static topology definitions. Instead, backends will retrieve the full topology (routing table) from the coordinator during `InitColonyShard` processing, not before. Since a single backend can host multiple shards (receiving multiple `InitColonyShard` calls), topology is retrieved only on the first call and then cached and reused for all subsequent shard initializations on that backend. This ensures backends only get topology information when it's actually needed and available, and avoids redundant retrieval for multiple shards on the same backend.

This subtask establishes the foundation for full dynamic topology elimination by ensuring backends retrieve topology from the coordinator rather than maintaining their own static copies. Topology creation will be made fully dynamic in a later phase.

## Motivation
- **Single Source of Truth**: Establish coordinator as the authoritative source for topology information
- **Eliminate Static Topology Duplication**: Remove static topology definitions from backend processes
- **Foundation for Dynamic Topology**: This is the first step toward full dynamic topology elimination as described in `dynamic_topology_elimination.md`
- **Deferred Topology Loading**: Backends only retrieve topology during shard initialization, not at startup
- **Cloud-Native Pattern**: Coordinator maintains topology, backends retrieve it when needed

## Current State Analysis

### Static Topology Usage

**Coordinator** (`crates/coordinator/src/cloud_start.rs`):
- Uses `ClusterTopology::initialize_with_dynamic_topology()` to create topology during cloud-start
- Creates shard map and distributes shards across discovered backends
- Maintains topology in memory after creation

**Backend** (`crates/backend/src/be_main.rs`, `crates/backend/src/be_ticker.rs`):
- Uses `ClusterTopology::get_instance()` to access static topology
- Uses topology for:
  - Self-validation at startup (localhost mode only)
  - Finding adjacent shards and their hosting backends during shard tick processing
- Currently gets topology from static constants via lazy initialization

**GUI** (`crates/gui/src/call_be.rs`, `crates/gui/src/gui_main.rs`):
- Uses `ClusterTopology::get_instance()` to access static topology
- Uses topology for:
  - Getting routing table (currently via RPC `GetRoutingTable`)
  - Determining which backend hosts which shard for image/layer requests
  - Getting shard dimensions for display configuration
- Currently gets topology from static constants via lazy initialization
- **Note**: GUI continues to use static topology in this phase (unchanged)

### Static Topology Constants
Located in `crates/shared/src/cluster_topology.rs`:
- `COORDINATOR_PORT: u16 = 8082`
- `BACKEND_PORTS: &[u16] = &[8084, 8086, 8088, 8090]`
- `HOSTNAME: &str = "127.0.0.1"`
- `WIDTH_IN_SHARDS: i32 = 8`
- `HEIGHT_IN_SHARDS: i32 = 5`
- `SHARD_WIDTH: i32 = 250`
- `SHARD_HEIGHT: i32 = 250`

### Current Initialization Pattern
1. **Lazy Initialization**: `ClusterTopology::get_instance()` uses `OnceLock` to lazily initialize with `new_fixed_topology()` on first access
2. **Cloud-Start Override**: `initialize_with_dynamic_topology()` allows pre-initialization for cloud-start mode, but only if called before first `get_instance()` call
3. **Static Method Access**: Many places call static methods like `get_width_in_shards()`, `get_shard_width()`, etc., which read from constants

## Proposed Changes

### 1. Coordinator: Provide Topology to Backend

**Change**: Coordinator will include topology (routing table) in the `InitColonyShardRequest` sent to backend. Topology creation remains unchanged:
- **Localhost mode**: Coordinator uses static topology (via `new_fixed_topology()` and constants)
- **AWS mode**: Coordinator creates topology in `cloud_start` (as it currently does)

**Implementation**:
- Topology creation logic remains unchanged (static for localhost, dynamic for AWS via `cloud_start`)
- Coordinator maintains topology in memory after creation (as it currently does)
- Coordinator includes routing table in `InitColonyShardRequest` when calling backend
- Coordinator populates routing table field in request from its in-memory topology

**Rationale**: Coordinator continues to create topology as it does now. By including routing table in the request, backend receives topology as part of the shard initialization flow without needing a separate RPC call.

### 2. Backend: Retrieve Topology During InitColonyShard

**Change**: Backends will no longer use `ClusterTopology::get_instance()` to access static topology. Instead, backends will retrieve the full routing table from the coordinator during `InitColonyShard` processing.

**Important**: A single backend can host multiple shards, so it will receive multiple `InitColonyShard` calls (one per shard). Topology should only be retrieved on the **first** `InitColonyShard` call and then reused for all subsequent calls.

**Implementation**:
- Remove `ClusterTopology::get_instance()` calls from backend startup code
- Remove self-validation that uses static topology (localhost mode check)
- In `handle_init_colony_shard()`:
  1. Backend receives `InitColonyShardRequest` from coordinator (which includes routing table)
  2. **Check if topology has already been retrieved** (for this backend instance):
     - If not yet retrieved: Extract routing table from `InitColonyShardRequest` and initialize topology
     - If already retrieved: Skip initialization and use cached topology (routing table in request can be ignored)
  3. Backend initializes `ClusterTopology` with the routing table from the request (cached for backend's lifetime)
  4. Backend validates that its own host info exists in the routing table (only on first retrieval)
  5. Backend proceeds with shard initialization using the retrieved/cached topology
- Backend uses cached topology for:
  - Finding adjacent shards and their hosting backends during shard tick processing
  - Any other topology-dependent operations
  - All subsequent `InitColonyShard` calls for additional shards hosted by this backend

**Error Handling**:
- If routing table is missing from request (on first call), return `InitColonyShardResponse::Error` (new response variant)
- Backend cannot proceed with shard initialization without topology

**Rationale**: Backends only need topology when they're actually hosting shards. Receiving topology in the `InitColonyShardRequest` ensures topology is available when needed and defers topology loading until necessary. Since a backend can host multiple shards, topology is extracted from the request and initialized once on the first call, then cached and reused for all shards hosted by that backend. Once initialized, topology is cached and reused for the backend's lifetime since topology cannot change.

### 2. Static Topology Access Restrictions

**Change**: Backend processes will no longer be able to access static topology via `ClusterTopology::get_instance()`. GUI and coordinator continue to use static topology.

**Implementation**:
- Keep `ClusterTopology::get_instance()` method for coordinator and GUI use (unchanged)
- Backend must use retrieved topology instead of static access
- Backend will initialize `ClusterTopology` from routing table retrieved during `InitColonyShard`

**Note**: This is a transitional step. In full dynamic topology elimination, `get_instance()` will be completely refactored. For this subtask, we're preventing backend from using static topology while keeping it available for coordinator and GUI.

## API Changes

### Backend RPC Changes

**`InitColonyShardRequest`** (modified):
- Add `routing_table` field containing the full routing table (shard-to-host mapping)
- Coordinator populates this field when sending the request
- Backend extracts routing table from this field to initialize topology

**`InitColonyShardResponse`** (add new variant):
- Add `Error` variant for cases where routing table is missing or invalid

### Backend Internal Changes

**Remove**:
- `ClusterTopology::get_instance()` calls from backend startup code
- Self-validation that uses static topology (localhost mode check in `be_main.rs`)

**Add**:
- Extract routing table from `InitColonyShardRequest` in `handle_init_colony_shard()`
- Topology initialization from routing table in request (cached after first retrieval)
- Check to only initialize topology on first `InitColonyShard` call (since backend can host multiple shards)
- Error handling for cases where routing table is missing or invalid
- Self-validation using routing table (validate backend's own host info exists in routing table)

## Impact Analysis

### Files Requiring Changes

**High Impact** (Core topology retrieval):
- `crates/backend/src/be_main.rs`: Remove static topology access, extract routing table from `InitColonyShardRequest` in `handle_init_colony_shard()`
- `crates/coordinator/src/init_colony.rs`: Update to include routing table in `InitColonyShardRequest` when sending to backend

**Medium Impact** (Topology usage):
- `crates/backend/src/be_ticker.rs`: Ensure topology is available (initialized during init) before use
- `crates/shared/src/cluster_topology.rs`: May need helper methods for initializing from routing table
- `crates/shared/src/be_api.rs`: Add `routing_table` field to `InitColonyShardRequest` structure

### Migration Path

**Phase 1: Coordinator - Include Routing Table in Request**
1. Add `routing_table` field to `InitColonyShardRequest` structure
2. Update coordinator to populate routing table field when sending `InitColonyShardRequest` to backend
3. Test coordinator correctly includes routing table in requests

**Phase 2: Backend Topology Retrieval**
1. Remove static topology access from backend startup
2. Extract routing table from `InitColonyShardRequest` in `handle_init_colony_shard()`
3. Add check to initialize topology only on first `InitColonyShard` call (since backend can host multiple shards)
4. Initialize topology from routing table in request (cache after first retrieval)
5. Add self-validation using routing table (only on first retrieval)
6. Test backend can process shard initialization with topology from request
7. Test backend correctly reuses cached topology for multiple shards on the same backend

## Success Criteria

✅ Backend no longer uses `ClusterTopology::get_instance()` for static topology access
✅ Coordinator includes routing table in `InitColonyShardRequest` sent to backend
✅ Backend extracts routing table from `InitColonyShardRequest` during `InitColonyShard` processing
✅ Backend caches topology after first retrieval and reuses it for subsequent operations
✅ Backend validates its own host info exists in the routing table from request
✅ Backend only initializes topology on first `InitColonyShard` call (reuses cached topology for additional shards)
✅ Topology creation remains unchanged (static for localhost, dynamic for AWS via `cloud_start`)
✅ GUI and coordinator continue to use static topology (unchanged)
✅ Backend can function correctly with topology received from coordinator in request

## Relationship to Dynamic Topology Elimination

This specification is the **first subtask** toward achieving the full "Dynamic Topology Elimination" goal described in `dynamic_topology_elimination.md`. 

**What this subtask achieves**:
- Establishes coordinator as single source of truth for topology
- Removes static topology access from backend
- Sets up pattern for topology retrieval from coordinator (backend retrieves during `InitColonyShard`)

## Notes

- This specification focuses on backend topology retrieval, not topology creation. Topology creation remains unchanged:
  - **Localhost mode**: Static topology (using constants in `cluster_topology.rs`)
  - **AWS mode**: Dynamic topology created in `cloud_start` (as it currently is)
- GUI and coordinator continue to use static topology (unchanged in this phase).
- No migration considerations are needed - this is a breaking change that requires backend to be updated.
- Backend topology retrieval happens during `InitColonyShard`, not at startup, ensuring topology is only loaded when needed.
- Coordinator includes routing table directly in `InitColonyShardRequest`, so backend receives topology as part of the request without needing a separate RPC call.
- Since a backend can host multiple shards, it will receive multiple `InitColonyShard` calls. Topology is extracted and initialized only on the first call (from the routing table in the request) and then cached and reused for all subsequent shard initializations on that backend.
- Backend caches topology after first initialization and reuses it for the backend's lifetime.
- Topology cannot change after initialization, so once cached, it remains stable for the backend's lifetime.
- Backend validates its own host info exists in the routing table (only on first initialization) to ensure it's correctly included in the topology.
