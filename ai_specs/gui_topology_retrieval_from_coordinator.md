# GUI Topology Retrieval from Coordinator

## Acknowledgment
Status: Pending approval

## Overview

This specification defines the second subtask toward achieving "Dynamic Topology Elimination". The goal is to make the GUI retrieve topology from the coordinator via HTTP API instead of using static topology definitions.

**What this phase changes**:
- **GUI topology retrieval**: GUI will discover the coordinator via SSM abstraction and retrieve the full `ClusterTopology` object from the coordinator via a new HTTP API endpoint. GUI must be run with a mode parameter (localhost or aws) to determine which cluster registry to use.
- **Coordinator HTTP API**: Coordinator will expose `GET /topology` endpoint that returns the full `ClusterTopology` object as JSON.

**What stays unchanged**:
- Topology creation (static for localhost, dynamic for AWS via `cloud_start`)
- Backend and coordinator operations (backend already retrieves topology from coordinator)

## Current State

**GUI** (`crates/gui/src/call_be.rs`, `crates/gui/src/gui_main.rs`):
- Uses `ClusterTopology::get_instance()` to access static topology
- Uses topology for routing table, shard dimensions, and shard endpoint discovery
- Uses RPC for coordinator communication (`GetRoutingTable` request)

**Coordinator** (`crates/coordinator/src/http_server.rs`):
- Has HTTP server but doesn't expose topology via HTTP API yet
- Has RPC endpoint for `GetRoutingTable` (used by GUI currently)

## Proposed Changes

### 1. Coordinator: Expose Topology via HTTP API

Add `GET /topology` endpoint that returns the full `ClusterTopology` object serialized as JSON. Response includes `coordinator_host`, `backend_hosts`, and `shard_to_host` fields. If topology is not initialized (colony not started), return `404 Not Found` with JSON error message: `{"error": "Topology not initialized"}`. Return `500 Internal Server Error` with error message if serialization fails.

### 2. GUI: Discover Coordinator and Retrieve Topology

**Implementation**:
- GUI accepts mode parameter (localhost or aws) and initializes appropriate `ClusterRegistry`
- At startup: discover coordinator via `ssm::discover_coordinator()` (uses cluster registry), extract HTTP port from `NodeAddress`, make HTTP GET to `http://{coordinator_ip}:{http_port}/topology`, deserialize JSON into `ClusterTopology`, cache in application state
- Replace static topology access with cached topology
- Calculate dimensions from shard mapping (width/height in shards from grid layout, shard dimensions from any shard)
- **Error handling**: Show error message and exit on any failure (coordinator discovery, HTTP request, or deserialization). No retry, no fallback to static topology.

**Rationale**: Topology never changes during runtime, so fetch once at startup and cache for GUI lifetime. HTTP API is cloud-friendly. SSM abstraction provides unified discovery for both modes.

## API Changes

**New Endpoint**: `GET /topology`
- Returns: `ClusterTopology` object as JSON (200 OK)
- Errors: 
  - 404 Not Found with JSON error `{"error": "Topology not initialized"}` if topology not initialized (colony not started)
  - 500 Internal Server Error with JSON error message if serialization fails
- Content-Type: `application/json`

**GUI Changes**:
- Remove: `ClusterTopology::get_instance()` calls, static dimension methods, RPC `GetRoutingTable`
- Add: Mode parameter, cluster registry initialization, coordinator discovery, HTTP GET to `/topology`, topology caching, dimension calculation, fail-fast error handling

## Impact Analysis

**Files Requiring Changes**:
- `crates/coordinator/src/http_server.rs`: Add `GET /topology` endpoint
- `crates/gui/src/gui_main.rs`: Add mode parameter, initialize cluster registry, remove static topology access, add topology retrieval
- `crates/gui/src/call_be.rs`: Remove static topology access, use cached topology
- `crates/shared/src/cluster_topology.rs`: May need helper methods to calculate dimensions from shard mapping

## Migration Path

**Phase 1**: Add `GET /topology` endpoint to coordinator, test returns correct topology and error handling.

**Phase 2**: Add mode parameter to GUI, initialize cluster registry, discover coordinator, fetch topology via HTTP, cache topology, replace static access, add error handling. Test in both localhost and aws modes.

## Success Criteria

✅ Coordinator exposes `GET /topology` returning full `ClusterTopology` as JSON
✅ GUI accepts mode parameter and initializes appropriate cluster registry
✅ GUI discovers coordinator via SSM abstraction and retrieves topology via HTTP API
✅ GUI caches topology for lifetime (no periodic refresh)
✅ GUI no longer uses static topology access or static dimension methods
✅ GUI calculates dimensions from shard mapping
✅ GUI shows error and exits on failures (no retry, no fallback)
✅ All components work correctly with retrieved topology
✅ `ClusterTopology::get_instance()` is only available within the coordinator crate

## Relationship to Dynamic Topology Elimination

This is the second subtask. It removes static topology access from GUI and establishes HTTP API pattern for topology retrieval. Backend already retrieves topology from coordinator. Future phases will make topology creation fully dynamic and eliminate static topology code.
