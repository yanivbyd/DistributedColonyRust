# Dynamic Topology Elimination

## Acknowledgment
Status: Pending review

## Overview
This specification defines a design to eliminate static topology configuration from the codebase. Currently, topology is defined through hardcoded constants (ports, hostnames, shard dimensions) that are initialized lazily on first access. The goal is to replace this with a fully dynamic topology system with the following flow:

1. **Coordinator Startup**: The coordinator starts without topology. Topology is **only created during the colony-start HTTP command** (`POST /cloud-start`), not automatically at coordinator startup. When the colony-start command is received:
   - Coordinator discovers available backend nodes via service discovery (`ClusterRegistry`)
   - Distributes shards evenly across those discovered backends
   - Creates the topology (shard-to-host mapping) in memory
   - Stores cluster configuration (shard dimensions) in `ClusterRegistry` for other processes to read
   - The coordinator maintains the authoritative topology in memory after creation

2. **GUI**: Currently uses RPC APIs, but the plan is to migrate to **HTTP APIs only** (from both coordinator and backend). This will make the GUI work easily with cloud deployments. GUI gets the topology (routing table) from the coordinator via HTTP API, then uses that information to access the correct backend nodes for each shard when making HTTP requests (e.g., `GetShardImage`, `GetShardLayer`). **Any new GUI calls must use HTTP API, not RPC.**

3. **Backends**: Discover the coordinator via `ClusterRegistry` and get cluster configuration. Backends need the full shard map to determine which shards are adjacent to the shards they host, and which backends host those adjacent shards, so they can send border updates during shard tick processing.

This makes the system more flexible and cloud-native, adapting automatically to the number of available backend nodes, with the coordinator as the single source of truth for topology. Topology creation is an explicit action triggered by the colony-start HTTP command, ensuring the system only initializes when explicitly requested.

## Motivation
- **Eliminate Hardcoded Constants**: Remove static configuration constants that limit flexibility
- **Support Dynamic Scaling**: Enable the system to adapt to varying numbers of backend nodes
- **Cloud-Native Architecture**: Align with cloud deployment patterns where topology is discovered, not hardcoded
- **Configuration Flexibility**: Support multiple configuration sources (environment variables, config files, service discovery)
- **Consistency**: Unify localhost and cloud deployment modes to use the same dynamic topology mechanism
- **Maintainability**: Reduce special cases and initialization order dependencies

## Current State Analysis

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
1. **Lazy Initialization**: `ClusterTopology::get_instance()` uses `OnceLock` to lazily initialize with `new_fixed_topology()`
2. **Cloud-Start Override**: `initialize_with_dynamic_topology()` allows pre-initialization for cloud-start mode, but only if called before first `get_instance()` call
3. **Static Method Access**: Many places call static methods like `get_width_in_shards()`, `get_shard_width()`, etc., which read from constants

### Usage Locations

#### 1. **Topology Instance Access**
- `crates/coordinator/src/init_colony.rs`: Gets backend hosts, shard-to-host mapping
- `crates/coordinator/src/global_topography.rs`: Gets host for shard
- `crates/coordinator/src/backend_client.rs`: Gets backend hosts for routing
- `crates/backend/src/be_ticker.rs`: Gets topology for shard distribution
- `crates/gui/src/call_be.rs`: Gets topology for shard endpoint resolution

#### 2. **Static Dimension Access**
- `crates/coordinator/src/init_colony.rs`: Uses `get_width_in_shards()`, `get_shard_width()`, `get_shard_height()` for colony initialization
- `crates/coordinator/src/cloud_start.rs`: Uses static methods to create shard map
- `crates/gui/src/gui_main.rs`: Uses static methods for GUI dimensions
- `crates/coordinator/src/global_topography.rs`: Uses dimensions for topography generation

#### 3. **Static Port/Hostname Access**
- `crates/shared/src/cluster_topology.rs`: `get_coordinator_port()`, `get_backend_ports()`, `get_hostname()` used in initialization
- Currently less critical since ports are now coming from env vars in AWS mode

### Existing Dynamic Mechanisms
- **DiscoveredTopology**: Already exists for service discovery via SSM
- **ClusterRegistry**: Abstraction for service registration/discovery (SSM for AWS, file-based for localhost)
- **initialize_with_dynamic_topology()**: Partial implementation for cloud-start mode

## Process Usage Analysis

This section explains how each process (coordinator, backend, GUI) currently uses the static topology, which is critical for understanding the impact of eliminating it.

### Coordinator Process

The coordinator is the **primary consumer** of topology information and uses it extensively:

#### 1. **Colony Initialization** (`init_colony.rs`)
- **Shard Discovery**: Gets all shards via `get_all_shards()` to initialize each shard on its assigned backend
- **Colony Dimensions**: Uses static methods `get_width_in_shards()`, `get_shard_width()`, `get_shard_height()` to:
  - Calculate total colony width/height for `InitColony` requests
  - Store dimensions in coordinator context
  - Generate topography information
- **Backend Host Discovery**: Gets all backend hosts via `get_all_backend_hosts()` to connect and initialize colony
- **Shard-to-Host Mapping**: Uses `get_host_for_shard()` to route shard initialization requests to the correct backend

#### 2. **Routing Table** (`coordinator_main.rs`)
- **GetRoutingTable Handler**: Builds routing table by iterating all shards and their assigned hosts
- Used by clients (GUI, other coordinators) to discover which backend hosts which shard

#### 3. **Colony Statistics** (`coordinator_main.rs`)
- **GetColonyStats Handler**: Gets all shards via `get_all_shards()` to aggregate statistics across the entire colony
- Iterates through all shards to collect metrics from each backend

#### 4. **Backend Communication** (`backend_client.rs`)
- **Shard Routing**: Uses `get_host_for_shard()` to route requests to the correct backend for a given shard
- **Backend Discovery**: Uses `get_all_backend_hosts()` to find unique backends for broadcasting events
- **Adjacent Shard Discovery**: Uses `get_backend_hosts_for_shards()` to find which backends host adjacent shards

#### 5. **Topography Generation** (`global_topography.rs`)
- **Shard-to-Host Mapping**: Uses `get_host_for_shard()` to send topography data to the correct backend for each shard
- **Dimension Access**: Uses static dimension methods indirectly through `GlobalTopographyInfo` structure

#### 6. **Cloud Start** (`cloud_start.rs`)
- **Dynamic Topology Initialization**: Uses `initialize_with_dynamic_topology()` to set topology before normal initialization
- **Shard Map Creation**: Uses static dimension methods (`get_width_in_shards()`, etc.) to create shard grid, then distributes shards across discovered backends

**Summary**: Coordinator is the **orchestrator** that needs complete topology knowledge (all backends, all shards, shard-to-host mapping, dimensions) to manage the colony.

### Backend Process

Backends use topology more **selectively**, primarily for validation and neighbor communication:

#### 1. **Startup Validation** (`be_main.rs`)
- **Localhost Mode Only**: Validates that the backend's own hostname and port exist in the static topology
- This is a **safety check** to ensure the backend is configured correctly in localhost mode
- Uses `get_all_backend_hosts()` to check if self is in the list

#### 2. **Shard Tick Processing** (`be_ticker.rs`)
- **Adjacent Shard Discovery**: Uses `get_adjacent_shards()` to find which shards are adjacent to updated shards
- **Neighbor Backend Discovery**: Uses `get_backend_hosts_for_shards()` to find which backends host adjacent shards
- **Update Propagation**: Sends shard updates to backends hosting adjacent shards
- **Self-Identification**: Compares own host info with topology to determine if updates should be sent externally

**Summary**: Backends need topology to:
- Validate their own configuration (localhost only)
- Find adjacent shards and their hosting backends for border update propagation
- **Require full shard map** to determine which shards are adjacent to hosted shards and which backends host those adjacent shards

### GUI Process

The GUI uses topology for **rendering and display**, but gets it from the coordinator rather than maintaining its own static copy. **Important**: GUI is currently using RPC APIs, but the plan is to migrate to **HTTP APIs only** (from both coordinator and backend) to work easily with cloud deployments. Any new GUI calls must use HTTP API, not RPC.

#### 1. **Topology Discovery** (Current: `call_be.rs`, Future: via coordinator HTTP API)
- **Current State**: Uses `ClusterTopology::get_instance()` to get static topology, uses RPC for `GetRoutingTable`
- **New Design**: Gets routing table from coordinator via HTTP API (e.g., `GET /routing-table`)
- **Topology Caching**: Builds local topology representation from routing table for efficient shard-to-host lookups

#### 2. **Display Configuration** (`gui_main.rs`)
- **Shard Grid Dimensions**: Uses static methods `get_width_in_shards()`, `get_height_in_shards()`, `get_shard_width()`, `get_shard_height()` to:
  - Calculate total display size
  - Configure shard grid layout (rows/columns)
  - Determine individual shard dimensions for rendering
- **New Design**: Gets dimensions from cluster configuration via HTTP API (e.g., `GET /cluster-config` from coordinator or ClusterRegistry)

#### 3. **Backend Communication** (`call_be.rs`)
- **Current State**: Uses RPC APIs to communicate with backends
- **New Design**: Uses HTTP APIs to communicate with backends
- **Shard Endpoint Resolution**: Uses topology to determine which backend to query for a specific shard
- **Image/Layer Requests**: Routes `GetShardImage` and `GetShardLayer` HTTP requests to the correct backend based on shard ownership from routing table

**Summary**: GUI needs topology to:
- Configure display dimensions (shard grid layout)
- Route requests to the correct backend for each shard
- Gets topology from coordinator (single source of truth) rather than maintaining static copy
- Uses HTTP APIs (not RPC) for all communication with coordinator and backends
- Does **not** need to modify topology or manage colony state

### Usage Patterns Summary

| Process | Primary Use Cases | Critical Dependencies |
|---------|------------------|----------------------|
| **Coordinator** | Colony initialization, routing table, statistics aggregation, backend communication, topography generation | Full topology (all backends, all shards, shard-to-host map, dimensions) |
| **Backend** | Self-validation (localhost), neighbor discovery for shard border updates | Full shard map (to find adjacent shards and their hosting backends), neighbor backends, self-validation |
| **GUI** | Display configuration, shard endpoint resolution | Gets topology from coordinator via HTTP API (not RPC), dimensions from cluster config via HTTP API |

### Initialization Order Dependencies

Currently, there's a **critical initialization order dependency**:

1. **Normal Startup**: `ClusterTopology::get_instance()` lazily initializes with `new_fixed_topology()` on first access
2. **Cloud-Start Mode**: `cloud_start_colony()` must call `initialize_with_dynamic_topology()` **before** any other code calls `get_instance()`
3. **Race Condition Risk**: If any code path calls `get_instance()` before cloud-start initialization, the static topology is locked in

This dependency makes the system fragile and requires careful coordination of initialization order.

## High-Level Design Decisions

### Decision 1: Configuration Source Strategy

**Decision**: **ClusterRegistry (SSM Abstraction)** - Extend `ClusterRegistry` trait to store and retrieve cluster configuration (shard dimensions). Store shard dimensions in ClusterRegistry (topology/shard-to-host mapping is kept in memory only). AWS: Store in SSM Parameter Store (via `SsmClusterRegistry`). Localhost: Store in file system (via `FileClusterRegistry`). Coordinator writes configuration during initialization, all processes read from ClusterRegistry. This provides a unified abstraction that works seamlessly for both AWS (SSM) and localhost (file-based) deployments, and centralizes cluster configuration in one place.

### Decision 2: Initialization Strategy

**Decision**: **Explicit Initialization** - Remove lazy initialization. Require explicit `ClusterTopology::initialize()` call at startup. Each process (coordinator/backend) initializes topology based on its role. Fail fast if topology cannot be initialized. This makes the system more predictable and easier to reason about.

### Decision 3: Shard Dimension Configuration

**Decision**: **ClusterRegistry** - Store shard dimensions in `ClusterRegistry` (SSM for AWS, file for localhost). Coordinator writes configuration during first initialization, all processes read from ClusterRegistry. Default values used only for initial colony creation if no config exists. This centralizes configuration and uses the same abstraction as node discovery.

### Decision 4: Backend Discovery Strategy

**Decision**: **Service Discovery Only** - All backends discovered via `ClusterRegistry`. No static backend list. Coordinator discovers backends at startup, backends discover coordinator at startup. For localhost, the file-based registry provides the static list. This unifies the approach across deployment modes.

### Decision 5: Shard-to-Host Mapping Strategy

**Decision**: **Coordinator-Determined** - Coordinator first discovers available backend nodes via `ClusterRegistry`, then creates and maintains the shard map by distributing shards evenly across discovered backends using round-robin. Map is deterministic based on the discovered backend list. This is consistent with current `cloud_start` implementation and provides centralized control while adapting to the actual number of available backends.

### Decision 6: Topology Update Strategy

**Decision**: **Static After Initialization** - Topology is fixed after initialization. Changes require process restart. Runtime scaling can be addressed in a future specification if needed. This keeps the current specification focused and manageable.

## Proposed Architecture

### 1. Configuration Sources

**ClusterRegistry (Extended)**:
- **Cluster Configuration**: Shard dimensions stored in ClusterRegistry
  - `width_in_shards`, `height_in_shards`, `shard_width`, `shard_height`
  - Stored at `/colony/config` (SSM) or `output/ssm/config.json` (file)
  - Coordinator writes configuration during initialization
  - All processes read configuration from ClusterRegistry
- **Node Discovery**: Existing ClusterRegistry functionality
  - Backend nodes discovered from SSM (AWS) or file registry (localhost)
  - Coordinator discovered from SSM (AWS) or file registry (localhost)

**Environment Variables** (per-process, already used):
- `RPC_PORT` (per-process, already used)
- `HTTP_PORT` (per-process, already used)
- `DEPLOYMENT_MODE` (localhost/aws)

**Note**: For initial colony creation, coordinator may use default dimension values if no configuration exists in ClusterRegistry yet. After initialization, configuration is stored in ClusterRegistry for all subsequent processes.

### 2. Initialization Flow

**Coordinator Startup** (before colony-start):
1. Coordinator starts and waits for HTTP requests
2. No topology is created at this stage
3. Coordinator is ready to receive `POST /cloud-start` command

**Colony-Start HTTP Command** (`POST /cloud-start`):
1. **Discover available backend nodes** via `ClusterRegistry` (this determines how many backends are available)
2. Read cluster configuration from `ClusterRegistry` (or use defaults if first initialization)
3. Create shard grid based on dimensions from configuration
4. **Distribute shards evenly across discovered backends** (round-robin distribution based on the number of backends found in step 1)
5. Store cluster configuration in `ClusterRegistry` (if not already present)
6. Initialize `ClusterTopology` with discovered topology (backends + shard-to-host mapping)
7. Proceed with colony initialization

**Backend Startup** (before colony initialization):
1. Discover coordinator via `ClusterRegistry`
2. Read cluster configuration from `ClusterRegistry` (required - must exist)
3. Initialize `ClusterTopology` with discovered coordinator, self, and configuration (no shard map yet)
4. Backend waits for colony initialization

**Backend Colony/Shard Initialization** (during `InitColony` or `InitColonyShard`):
1. Coordinator calls `InitColony` or `InitColonyShard` on backend
2. Backend gets full shard map (routing table) from coordinator via RPC (e.g., `GetRoutingTable` request) or as part of the init request
3. Backend updates `ClusterTopology` with full shard map
4. Backend uses full shard map to determine adjacent shards and their hosting backends for border updates

**GUI Startup**:
1. Discover coordinator via `ClusterRegistry`
2. Request routing table from coordinator via HTTP API (e.g., `GET /routing-table`)
3. Build local topology representation from routing table (shard-to-host mapping)
4. Get cluster configuration (dimensions) from coordinator via HTTP API (e.g., `GET /cluster-config`)
5. Use topology to route HTTP requests to correct backend nodes for each shard
6. **Note**: All GUI communication uses HTTP APIs (not RPC) for cloud compatibility

### 3. API Changes

**Remove Completely** (no fallback):
- **Delete** `ClusterTopology::new_fixed_topology()` - static topology implementation removed entirely
- **Delete** all static topology constants: `COORDINATOR_PORT`, `BACKEND_PORTS`, `HOSTNAME`, `WIDTH_IN_SHARDS`, `HEIGHT_IN_SHARDS`, `SHARD_WIDTH`, `SHARD_HEIGHT`
- **Remove** lazy initialization fallback: `ClusterTopology::get_instance()` will require explicit initialization first (no automatic fallback to static topology)
- **Remove** static methods: `get_backend_ports()`, `get_coordinator_port()`, `get_hostname()`
- **Remove** static dimension methods: `get_width_in_shards()`, `get_height_in_shards()`, `get_shard_width()`, `get_shard_height()`
- **Important**: Static topology code must be completely deleted - there is no fallback mechanism

**Add**:
- `ClusterTopology::initialize(config: TopologyConfig) -> Result<Arc<ClusterTopology>, TopologyError>` - explicit initialization required at startup
- `ClusterTopology::get_instance() -> Option<Arc<ClusterTopology>>` - returns None if not initialized (fail-fast)
- Instance methods for dimensions: `width_in_shards()`, `height_in_shards()`, `shard_width()`, `shard_height()`

**ClusterRegistry Extension**:
- Add `async fn get_cluster_config(&self) -> Option<ClusterConfig>` to `ClusterRegistry` trait
- Add `async fn set_cluster_config(&self, config: ClusterConfig) -> Result<(), String>` to `ClusterRegistry` trait
- Implement for both `SsmClusterRegistry` (store in `/colony/config`) and `FileClusterRegistry` (store in `output/ssm/config.json`)

**GUI Changes**:
- GUI no longer uses `ClusterTopology::get_instance()` directly
- GUI migrates from RPC APIs to **HTTP APIs only** (for both coordinator and backend communication)
- GUI gets topology from coordinator via HTTP API (e.g., `GET /routing-table`)
- GUI builds local topology representation from routing table for efficient lookups
- GUI gets cluster configuration (dimensions) from coordinator via HTTP API (e.g., `GET /cluster-config`)
- All new GUI calls must use HTTP API, not RPC
- This enables GUI to work easily with cloud deployments where HTTP is more accessible than RPC

**New Types**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub width_in_shards: i32,
    pub height_in_shards: i32,
    pub shard_width: i32,
    pub shard_height: i32,
}

pub struct TopologyConfig {
    pub cluster_config: ClusterConfig,
    pub coordinator_host: HostInfo,
    pub backend_hosts: Vec<HostInfo>,
    pub shard_to_host: HashMap<Shard, HostInfo>,
}
```

## Impact Analysis

### Files Requiring Changes

**High Impact** (Core topology logic):
- `crates/shared/src/cluster_topology.rs`: Complete refactor
- `crates/shared/src/cluster_registry.rs`: Extend trait to support cluster configuration storage/retrieval

**Medium Impact** (Initialization points):
- `crates/coordinator/src/coordinator_main.rs`: Add explicit initialization
- `crates/backend/src/be_main.rs`: Add explicit initialization
- `crates/coordinator/src/cloud_start.rs`: Update to use new API

**Medium Impact** (Dimension access):
- `crates/coordinator/src/init_colony.rs`: Replace static calls with instance methods
- `crates/coordinator/src/global_topography.rs`: Replace static calls
- `crates/gui/src/gui_main.rs`: Replace static calls

**Low Impact** (Topology access):
- `crates/coordinator/src/backend_client.rs`: Handle Option return
- `crates/backend/src/be_ticker.rs`: Handle Option return
- `crates/gui/src/call_be.rs`: Handle Option return

## Open Questions

1. **Topology Persistence**: Should topology (shard-to-host mapping) be persisted for high availability?
   - **Decision**: Not needed for now. Topology is kept in memory only. Storage/persistence can be added later when high availability is required.
   
2. **Shard Dimension Persistence**: Should shard dimensions be stored in coordinator context for persistence across restarts?
   - **Recommendation**: Dimensions are NOT stored in `ClusterRegistry`. It should be in coordinator context (but not stored for now, memory only)

3. **Backend Topology Awareness**: **Decision** - Backends need the full shard map to determine which shards are adjacent to the shards they host, and which backends host those adjacent shards. This is required for border update propagation during shard tick processing. Backends get the full shard map from coordinator during colony initialization (via `InitColony` or `InitColonyShard` RPC calls), not at startup, since the routing table is only created during the colony-start HTTP command. 

4. **Topology Validation**: Should we validate topology consistency (e.g., all shards assigned, no overlaps)?
   - **Recommendation**: Yes, add validation in `initialize()` method

5. **Error Handling**: **Decision** - Fail fast with clear error messages. **Static topology is completely removed** - there is no fallback mechanism. If topology cannot be initialized (e.g., no backends discovered, cluster config missing), the process must fail with a clear error message.

## Success Criteria

✅ No hardcoded topology constants in `cluster_topology.rs`
✅ All topology configuration (shard dimensions) stored in `ClusterRegistry` (SSM for AWS, file for localhost)
✅ Works in both localhost and AWS deployment modes using the same abstraction
✅ Clear error messages when topology cannot be initialized

## Notes

- This specification focuses on high-level design decisions. Detailed implementation tasks will be created separately.
- The existing `DiscoveredTopology` and `ClusterRegistry` mechanisms provide a good foundation for dynamic discovery.
- Port configuration is already moving toward environment variables (see port_canonization spec), which aligns with this effort.
- Shard dimensions are the main remaining static configuration that needs to be made dynamic.
- **Topology storage**: Topology (shard-to-host mapping) is kept in memory only and not persisted. Cluster configuration (shard dimensions) is stored in ClusterRegistry for other processes to read. Topology persistence can be added later when high availability is required.
- **GUI HTTP API migration**: GUI is currently using RPC APIs but must migrate to HTTP APIs only (from both coordinator and backend). This enables the GUI to work easily with cloud deployments. Any new GUI calls must use HTTP API, not RPC.
