# Dynamic Topology Elimination

## Spec Header

**Status**: draft

---

## Overview

This specification defines a design to eliminate static topology configuration from the codebase. Currently, topology is defined through hardcoded constants (ports, hostnames, shard dimensions) that are initialized lazily on first access. The goal is to replace this with a fully dynamic topology system.

**Current Implementation State**:
- Backends retrieve topology from coordinator during `InitColonyShard` (completed - see `backend_topology_retrieval_from_coordinator.md`)
- GUI retrieves topology via HTTP API from coordinator (completed - `GET /topology` endpoint)
- Static topology constants and methods still exist and are used by coordinator and cloud-start

**Remaining Work**:
- Remove static topology constants (`COORDINATOR_PORT`, `BACKEND_PORTS`, `WIDTH_IN_SHARDS`, etc.) - **no backward compatibility**
- Replace lazy initialization with explicit initialization (fail-fast if not initialized)
- Replace static dimension methods with instance methods
- Update GUI to handle 404 on `GET /topology` and automatically initiate `POST /cloud-start`
- Ensure coordinator never initializes topology automatically (always requires explicit cloud-start)

## Architecture

### Topology Flow

1. **Coordinator Startup**: Coordinator starts without topology. **Topology is never created automatically** - it must be created explicitly via `POST /cloud-start`:
   - Discovers available backend nodes via `ClusterRegistry` (node addresses and ports only)
   - Uses default shard dimensions for first initialization
   - Creates shard grid and distributes shards evenly across discovered backends
   - Stores cluster configuration (shard dimensions) in coordinator context (in memory only, never persisted to disk)
   - Initializes `ClusterTopology` in memory (topology contains shard dimensions via shard map)
   - `GET /topology` returns 404 if topology not initialized

2. **Backend Startup**: Backends discover coordinator via `ClusterRegistry` and retrieve topology during `InitColonyShard` (already implemented).

3. **GUI Startup**: GUI discovers coordinator via `ClusterRegistry` and attempts to retrieve topology via `GET /topology` HTTP endpoint:
   - If topology exists: GUI receives topology and proceeds normally
   - If topology not initialized (404 response): GUI automatically initiates `POST /cloud-start` to create topology, then retries `GET /topology`

### Configuration Storage

**ClusterRegistry Scope**:
- `ClusterRegistry` is only for node discovery (addresses and ports)
- Does NOT store cluster configuration (shard dimensions)

**Cluster Configuration Storage**:
- Shard dimensions are stored in coordinator context (in memory only, never persisted to disk)
- Dimensions can be derived from topology's shard map (shards contain width/height)
- For first initialization, coordinator uses default values
- Other processes (GUI, backends) get dimensions from topology via HTTP API or RPC

## Remaining Changes

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

### 4. Update Coordinator Cloud-Start

**Modify `cloud_start.rs`**:
- Use default shard dimensions for first initialization
- Store cluster configuration (shard dimensions) in coordinator context (in memory) after creating topology
- Use instance methods for dimensions instead of static methods
- Ensure no automatic topology initialization occurs at coordinator startup

### 5. Update GUI to Handle Missing Topology

**Modify `gui_main.rs`**:
- When `GET /topology` returns 404, automatically call `POST /cloud-start` (with generated idempotency key)
- After cloud-start completes, retry `GET /topology`
- Handle cloud-start errors gracefully (show error to user)

## Impact Analysis

**High Impact Files**:
- `crates/shared/src/cluster_topology.rs`: Remove static constants/methods, add explicit initialization
- `crates/coordinator/src/cloud_start.rs`: Use default dimensions or coordinator context, instance methods for dimensions

**Medium Impact Files**:
- `crates/coordinator/src/init_colony.rs`: Replace static dimension calls with instance methods
- `crates/coordinator/src/global_topography.rs`: Replace static dimension calls
- `crates/gui/src/gui_main.rs`: Replace static dimension calls, add cloud-start initiation on 404
- `crates/coordinator/src/coordinator_main.rs`: Ensure no automatic topology initialization

## Success Criteria

✅ No hardcoded topology constants in `cluster_topology.rs`
✅ Cluster configuration stored in coordinator context (in memory)
✅ Explicit initialization required (no lazy initialization fallback)
✅ All dimension access via instance methods
✅ Coordinator never initializes topology automatically (always requires explicit cloud-start)
✅ GUI automatically initiates cloud-start when topology is missing (404 response)
✅ Works in both localhost and AWS deployment modes
✅ Clear error messages when topology cannot be initialized
