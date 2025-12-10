# SSM Substitution and Replacement for Localhost

## Acknowledgment
Acknowdleged by Yaniv

## Overview
This feature creates a localhost-compatible replacement for AWS Systems Manager (SSM) Parameter Store. The goal is to make localhost development as similar to cloud deployment as possible by supporting SSM registration and discovery, even though coordinator and backend processes run as separate processes on the same machine. This is achieved by introducing a `ClusterRegistry` abstraction that provides a clean interface for service registration and discovery, with implementations for AWS SSM (cloud) and file-based storage (localhost). The static Topology configuration will remain unchanged for localhost mode.

## Motivation
- **Consistency**: Make localhost development environment mirror cloud deployment behavior
- **Testing**: Enable testing of SSM registration and discovery logic locally
- **Process Isolation**: Support coordinator and backend running as different processes that need to discover each other
- **Simple Approach**: Support SSM API while keeping static topology, then evolve to full dynamic discovery

## Requirements

### Functional Requirements
1. **ClusterRegistry Abstraction**: Create a `ClusterRegistry` trait that abstracts service registration and discovery:
   - Provides clean interface independent of underlying implementation (SSM, file-based)
   - Methods: `register_coordinator()`, `register_backend()`, `discover_coordinator()`, `discover_backends()`, `unregister_coordinator()`, `unregister_backend()`
   - Implementations: `SsmClusterRegistry` (AWS), `FileClusterRegistry` (localhost)

2. **Registration and Discovery API**: Support registration and discovery operations:
   - Write coordinator address to ClusterRegistry
   - Write backend addresses to ClusterRegistry
   - Read coordinator address from ClusterRegistry
   - Read backend addresses from ClusterRegistry

3. **NodeAddress Extension**: Replace single `port` field in `NodeAddress` struct with two ports:
   - Remove existing `port: u16` field
   - Add `internal_port: u16` - Port for internal TCP communication (coordinator/backend protocol)
   - Add `http_port: u16` - Port for external HTTP REST API
   - Update all registration and discovery code to use both ports

4. **Multi-Process Support**: Coordinator and backend processes must be able to:
   - Register themselves independently with both ports
   - Discover each other's addresses (including both ports)
   - Work correctly even though they run as separate processes

5. **Compatibility**: 
   - Static Topology remains unchanged for localhost mode
   - Cloud deployment behavior unchanged

6. **Initial Implementation**: 
   - When in AWS mode: write/read to/from actual SSM
   - When in localhost mode: use file-based storage
   - Static Topology configuration stays in place
   - No changes to how topology is used in localhost mode

### Technical Requirements

#### SSM API Surface
The current SSM API consists of:
- **Reading**:
  - `discover_coordinator() -> Option<NodeAddress>` - reads `/colony/coordinator` parameter
  - `discover_backends() -> Vec<NodeAddress>` - reads all parameters under `/colony/backends/` path
- **Writing** (currently only via AWS CLI):
  - Coordinator: `aws ssm put-parameter --name "/colony/coordinator" --value "{\"ip\":\"IP\",\"internal_port\":PORT1,\"http_port\":PORT2}"`
  - Backend: `aws ssm put-parameter --name "/colony/backends/INSTANCE_ID" --value "{\"ip\":\"IP\",\"internal_port\":PORT1,\"http_port\":PORT2}"`
  - Format: JSON string `{"ip":"IP","internal_port":PORT1,"http_port":PORT2}`

#### NodeAddress Structure Extension
- **Current**: `NodeAddress` has `ip: String` and `port: u16` (single port)
- **New**: Replace `port` field with two ports:
  - Remove `port: u16` field
  - Add `internal_port: u16` - Port for internal communication (TCP protocol for coordinator/backend communication)
  - Add `http_port: u16` - Port for external communication (HTTP REST API)
- This allows nodes to expose both internal protocol endpoints and HTTP API endpoints
- Both ports are required for registration and discovery

#### Localhost Replacement Options

**Option 1: File-Based Storage (Recommended)**
- Store coordinator and backend addresses in JSON files in a shared directory (e.g., `output/ssm/`)
- Coordinator writes to `output/ssm/coordinator.json`
- Backends write to `output/ssm/backends/{instance_id}.json`
- Reading scans the directory structure
- **Pros**: 
  - Simple, no external dependencies
  - Works across processes via filesystem
  - No Docker required
  - Lightweight solution
  - Zero setup overhead
  - Keeps localhost mode simple and self-contained
- **Cons**: 
  - File I/O overhead
  - Potential race conditions (acceptable for now)
  - Cleanup needed on shutdown
  - Custom implementation required
  - Not using real SSM API (different code path)

#### Recommended Approach: File-Based Storage

**Primary Recommendation: File-Based Storage**
- Use file-based storage as the primary localhost SSM replacement
- Simple, zero-dependency solution that keeps localhost mode lightweight
- No Docker or external services required
- Custom implementation that matches our specific needs
- Works seamlessly across separate processes via filesystem

## High-Level Design

### Architecture

```
┌─────────────────┐         ┌──────────────────┐
│   Coordinator   │────────▶│ ClusterRegistry  │
│    Process      │         │   (Trait/API)    │
│   Backend       │         │  (shared crate)  │
│   Process       │         └──────────────────┘
└─────────────────┘                 │
                                     │
                          ┌──────────────────────┐
                          │  Implementation      │
                          │  Selection           │
                          │    (AWS/File)        │
                          └──────────────────────┘
                                     │
                    ┌────────────────┴────────────────┐
                    ▼                                  ▼
         ┌──────────────────┐              ┌──────────────┐
         │ SsmClusterReg.   │              │FileClusterReg│
         │ (AWS SSM)        │              │ (Primary)    │
         │                  │              │ (Localhost)  │
         └──────────────────┘              └──────────────┘
                    │                                  │
                    ▼                                  ▼
         ┌──────────────────┐              ┌──────────────┐
         │  AWS SSM         │              │ output/ssm/  │
         │  Parameter Store│              │ coordinator. │
         │                  │              │ json         │
         │                  │              │ backends/*   │
         └──────────────────┘              └──────────────┘
```

### Component Changes

#### 1. ClusterRegistry Module (`crates/shared/src/cluster_registry.rs` - new file)
- **ClusterRegistry Trait**:
  ```rust
  pub trait ClusterRegistry: Send + Sync + 'static {
      async fn register_coordinator(&self, address: NodeAddress) -> Result<(), Error>;
      async fn register_backend(&self, instance_id: String, address: NodeAddress) -> Result<(), Error>;
      async fn discover_coordinator(&self) -> Option<NodeAddress>;
      async fn discover_backends(&self) -> Vec<NodeAddress>;
      async fn unregister_coordinator(&self) -> Result<(), Error>;
      async fn unregister_backend(&self, instance_id: String) -> Result<(), Error>;
  }
  ```

- **Implementations**:
  - `SsmClusterRegistry` - Implements ClusterRegistry using AWS SSM Parameter Store
  - `FileClusterRegistry` - Implements ClusterRegistry using file-based storage (localhost)

- **Registry Factory**:
  - `create_cluster_registry(deployment_mode: DeploymentMode) -> Arc<dyn ClusterRegistry>`
  - Selects appropriate implementation based on deployment mode and environment variables
  - Returns singleton instance of the selected registry
  - `get_instance() -> Arc<dyn ClusterRegistry>` - Get the active registry instance
  - **Initialization**: ClusterRegistry should be initialized early in the process lifecycle, before any registration/discovery calls. This can be done in `main()` functions of coordinator and backend, or via lazy initialization on first use.

#### 2. SSM Module (`crates/shared/src/ssm.rs`)
- **Refactor to use ClusterRegistry**:
  - Replace direct SSM calls with ClusterRegistry abstraction
  - Keep existing `discover_coordinator()` and `discover_backends()` functions for backward compatibility
  - These functions will delegate to the active ClusterRegistry instance

- **Update NodeAddress Structure**:
  - Remove existing `port: u16` field
  - Replace with `internal_port: u16` and `http_port: u16` fields
  - Update `NodeAddress::new()` to accept both ports: `new(ip: String, internal_port: u16, http_port: u16)`
  - Add helper methods: `to_internal_address()` and `to_http_address()`
  - Update `parse_address()` to parse JSON format with both ports

- **FileClusterRegistry Implementation**:
  - Implement `ClusterRegistry` trait for file-based storage
  - Use `std::fs` for file operations
  - Handle directory creation automatically
  - Store coordinator at `output/ssm/coordinator.json`
  - Store backends at `output/ssm/backends/{instance_id}.json`

- **SsmClusterRegistry Implementation**:
  - Implement `ClusterRegistry` trait using `aws-sdk-ssm`
  - Use default AWS endpoint for cloud deployments
  - Serialize/deserialize NodeAddress to/from JSON format in SSM parameters
  - Note: Bash scripts may need to read SSM parameters to determine IP and ports, so JSON format should be parseable with standard tools (e.g., `jq`)

#### 3. Coordinator Main (`crates/coordinator/src/coordinator_main.rs`)
- **Registration on Startup**:
  - After binding to both ports (internal TCP and HTTP), register coordinator address
  - Create `NodeAddress` with both `internal_port` (TCP protocol port) and `http_port` (HTTP API port)
  - Get ClusterRegistry instance via `cluster_registry::get_instance()`
  - Use `cluster_registry.register_coordinator(address)` with address containing both ports
  - Handle registration errors gracefully (log but don't fail startup)

- **Cleanup on Shutdown**:
  - Register signal handlers for graceful shutdown
  - Get ClusterRegistry instance and call `unregister_coordinator()` on shutdown
  - Use `tokio::signal` for signal handling

#### 4. Backend Main (`crates/backend/src/be_main.rs`)
- **Registration on Startup**:
  - After initialization, register backend address with both ports
  - Create `NodeAddress` with both `internal_port` (TCP protocol port) and `http_port` (HTTP API port)
  - Generate instance ID using `internal_port`: `backend_{internal_port}` (e.g., `backend_8082`)
  - Get ClusterRegistry instance via `cluster_registry::get_instance()`
  - Use `cluster_registry.register_backend(instance_id, address)` with address containing both ports
  - Handle registration errors gracefully

- **Cleanup on Shutdown**:
  - Register signal handlers
  - Get ClusterRegistry instance and call `unregister_backend(instance_id)` on shutdown

#### 5. File Structure
```
output/
└── ssm/
    ├── coordinator.json          # {"ip": "127.0.0.1", "internal_port": 8083, "http_port": 8084}
    └── backends/
        ├── backend_8082.json    # {"ip": "127.0.0.1", "internal_port": 8082, "http_port": 8084}
        ├── backend_8084.json    # {"ip": "127.0.0.1", "internal_port": 8084, "http_port": 8086}
        └── ...
```

## Implementation Details

### NodeAddress Structure Changes

**Current Structure:**
```rust
pub struct NodeAddress {
    pub ip: String,
    pub port: u16,  // Single port (to be removed)
}
```

**New Structure:**
```rust
pub struct NodeAddress {
    pub ip: String,
    pub internal_port: u16,  // Port for internal TCP communication (replaces `port`)
    pub http_port: u16,      // Port for external HTTP REST API
}
```

**Implementation Considerations:**
- Remove `port: u16` field from `NodeAddress`
- Replace with `internal_port: u16` and `http_port: u16` fields
- Update `NodeAddress::new()` to accept both ports: `new(ip: String, internal_port: u16, http_port: u16)`
- Update `to_address()` method - may need separate methods:
  - `to_internal_address() -> String` - Returns `ip:internal_port`
  - `to_http_address() -> String` - Returns `ip:http_port`
  - Or keep `to_address()` returning internal address for existing code
- Update `parse_address()` to parse JSON format with both ports

### File Format
Each registration file will be a JSON object with both ports:
```json
{
  "ip": "127.0.0.1",
  "internal_port": 8083,
  "http_port": 8084
}
```

### SSM Parameter Format
When writing to SSM (AWS), the parameter value will be a JSON string:
- Format: `{"ip":"127.0.0.1","internal_port":8083,"http_port":8084}`

The `parse_address()` function in `ssm.rs` will:
1. Parse the JSON string to extract `ip`, `internal_port`, and `http_port`
2. Create and return `NodeAddress` with both ports

### Instance ID Generation
For localhost backends, use port-based instance IDs:
- Format: `backend_{internal_port}` (use `internal_port`, not `http_port`)
- Example: Internal port 8082 → `backend_8082`
- This ensures uniqueness and readability
- The `internal_port` is used because it's the primary identifier for backend communication

### Error Handling
- **Registration failures**: Log error but don't fail startup (graceful degradation)
- **Discovery failures**: Return empty results, log warning
- **File I/O errors**: Log error, return appropriate error type

### Deployment Mode Detection
- Check `DEPLOYMENT_MODE` environment variable (or command-line argument)
- Default to localhost if not set
- For localhost mode:
  - Use file-based provider (primary, default)
- For AWS mode:
  - Use default AWS SSM endpoint with `aws-sdk-ssm`

## Implementation Phases

### Phase 1: Extend NodeAddress Structure
1. Remove `port: u16` field from `NodeAddress` struct in `cluster_topology.rs`
2. Add `internal_port: u16` and `http_port: u16` fields to `NodeAddress`
3. Update `NodeAddress::new()` to accept both ports: `new(ip: String, internal_port: u16, http_port: u16)`
4. Add helper methods:
   - `to_internal_address() -> String` - Returns `ip:internal_port` for TCP protocol communication
   - `to_http_address() -> String` - Returns `ip:http_port` for HTTP REST API
5. Update `to_address()` method (returns internal address for existing code compatibility)
6. Update `parse_address()` in `ssm.rs` to parse JSON format with both ports
7. Update all existing code that creates `NodeAddress` to provide both ports:
   - Files that need updates: `cloud_start.rs`, `http_server.rs` (coordinator & backend), `be_main.rs`, 
     `backend_communication.rs`, `connection_pool.rs`, `backend_client.rs`, `global_topography.rs`, 
     `call_be.rs`, and test files
   - For each usage, determine which port is needed (internal vs http) and update accordingly
   - Most TCP protocol communication should use `internal_port`
   - HTTP API calls should use `http_port`

### Phase 2: Create ClusterRegistry Trait and Basic Structure
1. Create `ClusterRegistry` trait with all required methods
2. Create basic `FileClusterRegistry` and `SsmClusterRegistry` structs (implementations can be stubs initially)
3. Create registry factory functions (`create_cluster_registry()`, `get_instance()`)
4. Update `ssm.rs` to delegate `discover_coordinator()` and `discover_backends()` to ClusterRegistry
5. No registration calls yet - just API structure preparation

### Phase 3: ClusterRegistry and File-Based Provider Implementation
1. Create `ClusterRegistry` trait in `cluster_registry.rs`
2. Create `FileClusterRegistry` implementation of `ClusterRegistry` trait
3. Create `SsmClusterRegistry` implementation of `ClusterRegistry` trait
4. Add registry factory functions for creating and getting registry instances
5. Implement file I/O operations in `FileClusterRegistry`
6. Create `output/ssm/` directory structure automatically
7. Update `discover_*` functions in `ssm.rs` to delegate to ClusterRegistry
8. Test file-based discovery

### Phase 4: Registration Integration
1. Add registration calls in coordinator startup
2. Add registration calls in backend startup
3. Add cleanup on shutdown (signal handlers)
4. Test multi-process registration and discovery

### Phase 5: Testing and Validation
1. Test coordinator registration and discovery with file-based provider
2. Test backend registration and discovery with file-based provider
3. Test cleanup on shutdown
4. Test concurrent access (multiple backends registering simultaneously)
5. Verify static topology still works in localhost mode

## Files to Modify

### Shared Crate
- `crates/shared/src/cluster_topology.rs` - Replace `port` field with `internal_port` and `http_port` in `NodeAddress` struct
- `crates/shared/src/cluster_registry.rs` - New file: ClusterRegistry trait and implementations
- `crates/shared/src/ssm.rs` - Refactor to use ClusterRegistry, update `parse_address()` for two-port format
- `crates/shared/src/lib.rs` - Export `cluster_registry` module
- `crates/shared/Cargo.toml` - No additional dependencies needed

### Coordinator
- `crates/coordinator/src/coordinator_main.rs` - Add registration on startup and cleanup on shutdown

### Backend
- `crates/backend/src/be_main.rs` - Add registration on startup and cleanup on shutdown

### New Files
- `crates/shared/src/cluster_registry.rs` - ClusterRegistry trait and implementations (FileClusterRegistry, SsmClusterRegistry)

## Testing Strategy

### Unit Tests
- Test `NodeAddress` structure with both ports
- Test `to_internal_address()` and `to_http_address()` methods
- Test `parse_address()` with JSON format (two ports)
- Test file-based provider read/write operations with two-port format
- Test instance ID generation
- Test error handling

## Implementation Alternatives

### LocalStack as ClusterRegistry Implementation

LocalStack can be used as an alternative implementation of the `ClusterRegistry` abstraction. It provides a full AWS SSM Parameter Store API emulation that matches the ClusterRegistry interface, making it a drop-in replacement for testing or development scenarios where full SSM API compatibility is desired.

**How it works:**
- LocalStack runs in a Docker container and emulates AWS services locally
- A `LocalStackClusterRegistry` implementation would implement the `ClusterRegistry` trait using `aws-sdk-ssm` configured to point to LocalStack's endpoint (`http://localhost:4566`)
- This implementation matches the ClusterRegistry abstraction, providing the same interface as `SsmClusterRegistry` and `FileClusterRegistry`
- All ClusterRegistry methods (`register_coordinator()`, `discover_backends()`, etc.) work identically through the abstraction

**Benefits:**
- Full SSM API compatibility for testing
- Can replicate real AWS parameters locally
- Uses the same `aws-sdk-ssm` crate as production

**Note:** This is an implementation alternative that fits within the ClusterRegistry abstraction. The primary implementation for localhost is file-based storage, which requires no external dependencies.

## Error Handling

### Registration Errors
- **File write failures**: Log error, continue startup (don't fail)
- **Directory creation failures**: Log error, attempt to continue

### Discovery Errors
- **File read failures**: Return empty result, log warning
- **Invalid JSON**: Log error, skip that entry, continue with others
- **Missing files**: Return empty result (normal during startup)

### Cleanup Errors
- **File deletion failures**: Log warning, continue shutdown

## Logging

### Registration
- Log successful registration: `"Registered coordinator in ClusterRegistry: 127.0.0.1:8083 (internal), 127.0.0.1:8084 (http)"`
- Log registration failures: `"Failed to register coordinator: {error}"`

### Discovery
- Log discovery results: `"ClusterRegistry: discovered {} backend entries"` (existing format, updated name)
- Log file-based discovery: `"File ClusterRegistry: discovered {} backend entries"`

### Cleanup
- Log successful cleanup: `"Unregistered coordinator from ClusterRegistry"`
- Log cleanup failures: `"Failed to unregister coordinator: {error}"`

