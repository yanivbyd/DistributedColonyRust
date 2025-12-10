# Port Canonization for Localhost & Cloud

## Acknowledgment
Yaniv: ack

## Overview
This specification defines a canonical approach for port number management across localhost and cloud deployments. Each node requires two distinct ports: one for internal TCP communication (coordinator/backend protocol) and one for external HTTP REST API. The port assignment strategy differs between localhost (where all processes share the same machine) and AWS cloud (where each spot instance has constant port assignments).

## Motivation
- **Port Canonization**: Establish clear, consistent port assignment rules for both deployment modes
- **Variable Name Canonization**: Replace service-specific variable names (`BACKEND_PORT`, `COORDINATOR_PORT`) with purpose-based names (`RPC_PORT`, `HTTP_PORT`)
- **Separation of Concerns**: Distinguish between internal protocol ports and external HTTP API ports
- **Localhost Multi-Process Support**: Enable multiple processes on the same machine to use distinct ports for both internal and HTTP communication
- **Cloud Consistency**: Maintain constant port assignments per AWS spot instance
- **Script Configuration**: Update localhost running scripts to explicitly define ports for each process

## Requirements

### Functional Requirements

1. **Dual Port Architecture**: Each node (coordinator and backend) must have:
   - **RPC Port**: Used for TCP-based coordinator/backend RPC protocol communication
   - **HTTP Port**: Used for HTTP REST API endpoints (debug, cloud-start, etc.)

2. **Localhost Port Assignment**:
   - All processes run on the same machine (`127.0.0.1`)
   - Each process must have **unique** RPC and HTTP ports
   - Ports must be explicitly defined in the running script (`build_and_run.sh`)
   - No port conflicts between processes

3. **AWS Cloud Port Assignment**:
   - Each spot instance has **constant** port assignments per instance
   - **Coordinator**: RPC port from `RPC_PORT` env var (set to `8082`), HTTP port from `HTTP_PORT` env var (set to `8083`)
   - **Backend**: RPC port from `RPC_PORT` env var (set to `8084`), HTTP port from `HTTP_PORT` env var (set to `8085`)
   - Port assignments are consistent across instance lifecycle
   - Note: Each instance type sets `RPC_PORT` and `HTTP_PORT` to its specific values. Coordinator instances set `RPC_PORT=8082` and `HTTP_PORT=8083`. Backend instances set `RPC_PORT=8084` and `HTTP_PORT=8085`. `HTTP_PORT` is for HTTP REST API endpoints, `RPC_PORT` is for TCP-based coordinator/backend RPC protocol communication.

4. **Script Configuration**:
   - `build_and_run.sh` must define both RPC and HTTP ports for each backend process
   - Coordinator ports must also be explicitly defined
   - Port configuration should be clear and easy to modify

### Technical Requirements

#### Current State Analysis

**Localhost Mode (Current)**:
- Backend: Uses same port for both internal and HTTP (e.g., backend on 8082 uses 8082 for both)
- Coordinator: Uses 8083 for internal, 8083 for HTTP (same port)
- Issue: No separation between internal and HTTP ports

**AWS Mode (Current)**:
- Coordinator: RPC port from `COORDINATOR_PORT` env var (typically `8083`), HTTP port fixed at `8084`
- Backend: RPC port from `BACKEND_PORT` env var (typically `8082`), HTTP port fixed at `8084`
- Status: Already has separation, but uses legacy variable names (`BACKEND_PORT`, `COORDINATOR_PORT`) and hardcoded HTTP port
- **Target State**: Use canonical variable names `RPC_PORT` and `HTTP_PORT` for both backend and coordinator. Coordinator uses fixed ports (8082/8083), backends start from 8084 onwards. No migration needed - direct replacement of variable names and port assignments.
- Note: `HTTP_PORT` is for HTTP REST API endpoints, `RPC_PORT` is for TCP-based coordinator/backend RPC protocol communication

#### Port Assignment Strategy

**Sequential Port Pairs**

Use sequential port pairs for localhost processes:
- Coordinator uses fixed ports: RPC=8082, HTTP=8083 (ports remain constant regardless of number of backends)
- Backends start from 8084 onwards, each getting two consecutive ports: RPC and HTTP
- Port pairs are non-overlapping
- Easy to configure and understand
- Maintains clear separation between RPC and HTTP communication
- Coordinator ports stay the same when adding/removing backend nodes

**Port Assignment Table (Localhost)**:

| Process | RPC Port | HTTP Port |
|---------|----------|-----------|
| Coordinator | 8082 | 8083 |
| Backend 1 | 8084 | 8085 |
| Backend 2 | 8086 | 8087 |
| Backend 3 | 8088 | 8089 |
| Backend 4 | 8090 | 8091 |

**Port Assignment Table (AWS Cloud)**:

| Process Type | RPC Port | HTTP Port |
|--------------|---------|-----------|
| Coordinator | From `RPC_PORT` env var (set to `8082`) | From `HTTP_PORT` env var (set to `8083`) |
| Backend | From `RPC_PORT` env var (set to `8084`) | From `HTTP_PORT` env var (set to `8085`) |

Note: The `HTTP_PORT` is used for HTTP REST API endpoints. The `RPC_PORT` is used for TCP-based coordinator/backend RPC protocol communication. Both backend and coordinator use the same environment variable names (`RPC_PORT` and `HTTP_PORT`), with different values per instance type. Coordinator instances set `RPC_PORT=8082` and `HTTP_PORT=8083`. Backend instances set `RPC_PORT=8084` and `HTTP_PORT=8085`. Coordinator uses fixed ports (8082/8083) that remain constant regardless of the number of backend instances.

#### Implementation Details

1. **Update `build_and_run.sh`**:
   - Define port pairs for each backend process
   - Define coordinator port pair
   - Add port validation to check for conflicts before starting processes
   - Pass both ports to each process via command-line arguments

2. **Update Backend Main (`be_main.rs`)**:
   - Accept both RPC and HTTP ports as command-line arguments (format: `<hostname> <rpc_port> <http_port> <deployment_mode>`)
   - Add port validation to check if ports are available before binding
   - Use RPC port for TCP listener (RPC protocol)
   - Use HTTP port for HTTP server (in both localhost and AWS modes)
   - Register with both ports in `NodeAddress`

3. **Update Coordinator Main (`coordinator_main.rs`)**:
   - Accept both RPC and HTTP ports as command-line arguments (format: `<rpc_port> <http_port> <deployment_mode>`)
   - Add port validation to check if ports are available before binding
   - Use RPC port for TCP listener (RPC protocol)
   - Use HTTP port for HTTP server (in both localhost and AWS modes)
   - Register with both ports in `NodeAddress`

4. **Update Cluster Topology**:
   - Update `BACKEND_PORTS` constant in `cluster_topology.rs` to reflect new RPC ports (8084, 8086, 8088, 8090)
   - Update `COORDINATOR_PORT` constant to new RPC port (8082)
   - Ensure topology matches script configuration
   - Verify all references to port constants are updated

5. **Update AWS Configuration**:
   - **Canonize variable names**: Replace `BACKEND_PORT` and `COORDINATOR_PORT` with `RPC_PORT` (no migration needed - direct replacement)
   - **Canonize HTTP port**: Replace hardcoded HTTP port values with `HTTP_PORT` env var (no migration needed - direct replacement)
   - Both backend and coordinator use `RPC_PORT` and `HTTP_PORT` environment variables
   - Coordinator instances: Set `RPC_PORT=8082` and `HTTP_PORT=8083` in CDK/user-data
   - Backend instances: Set `RPC_PORT=8084` and `HTTP_PORT=8085` in CDK/user-data
   - Update CDK configuration (`cdk.json`, `user-data-builder.ts`) to use new variable names
   - Update Docker/container configuration to use new variable names
   - Update user-data scripts to set `RPC_PORT` and `HTTP_PORT` instead of legacy names

6. **Add Port Validation**:
   - Implement port availability checking before binding
   - Check both RPC and HTTP ports for conflicts
   - **Validation approach**: Attempt to bind to each port before starting the service. If binding fails, check if the port is already in use (e.g., using `lsof` or `netstat` on Unix systems, or checking for `AddressAlreadyInUse` errors). Provide clear error messages indicating which port is in use and by which process if possible.
   - In localhost mode: validate all ports are unique across processes before starting any process
   - In AWS mode: validate ports are available on the instance before binding
   - Fail fast: Exit with clear error message if ports are unavailable rather than attempting to start with invalid ports

7. **Update Kill Scripts**:
   - Update `kill_all.sh` to handle new port numbers
   - Ensure all ports are properly released

## Implementation Plan

### Phase 1: Update Script Configuration
1. Modify `build_and_run.sh` to define port pairs for each process
2. Update command-line invocations to pass both ports
3. Add port validation to check for conflicts before starting processes
4. Update `kill_all.sh` to handle new port numbers

### Phase 2: Update Backend Code
1. Modify `be_main.rs` to accept RPC and HTTP ports as command-line arguments (format: `<hostname> <rpc_port> <http_port> <deployment_mode>`)
2. Add port validation to check if ports are available before binding
3. Start HTTP server in both localhost and AWS modes using HTTP port
4. Update `NodeAddress` registration with both ports

### Phase 3: Update Coordinator Code
1. Modify `coordinator_main.rs` to accept RPC and HTTP ports as command-line arguments (format: `<rpc_port> <http_port> <deployment_mode>`)
2. Add port validation to check if ports are available before binding
3. Start HTTP server in both localhost and AWS modes using HTTP port
4. Update `NodeAddress` registration with both ports

### Phase 4: Update Cluster Topology
1. Update `BACKEND_PORTS` constant in `cluster_topology.rs` to reflect new RPC ports
2. Update `COORDINATOR_PORT` constant to new RPC port
3. Verify topology matches script configuration

### Phase 5: Canonize Environment Variable Names
1. Replace `BACKEND_PORT` with `RPC_PORT` in backend code and configuration (no migration needed - direct replacement)
2. Replace `COORDINATOR_PORT` with `RPC_PORT` in coordinator code and configuration (no migration needed - direct replacement)
3. Replace hardcoded HTTP port values with `HTTP_PORT` env var (no migration needed - direct replacement)
4. Update CDK configuration (`cdk.json`, `user-data-builder.ts`) to use new variable names
5. Update Docker configuration to use new variable names
6. Update all scripts and documentation to reference new variable names

## Variable Name Canonization

### Current Variable Names (Legacy)
- `BACKEND_PORT`: Backend RPC port (typically `8082`)
- `COORDINATOR_PORT`: Coordinator RPC port (typically `8083`)
- HTTP port: Hardcoded as `8084` in code

### Target Variable Names (Canonical)
- `RPC_PORT`: RPC port for TCP-based coordinator/backend RPC protocol communication (used by both backend and coordinator)
- `HTTP_PORT`: HTTP port for HTTP REST API endpoints (used by both backend and coordinator)

### Benefits of Canonization
- **Consistency**: Same variable names for both backend and coordinator
- **Clarity**: Names clearly indicate purpose (RPC vs HTTP)
- **Flexibility**: HTTP port is configurable, not hardcoded
- **Simplicity**: Fewer special-case variable names to remember

### Implementation Approach
- **No Migration Needed**: This is a direct replacement of variable names and port assignments, not a migration
- **Direct Replacement**: No backward compatibility with legacy variable names - all code must be updated to use `RPC_PORT` and `HTTP_PORT` in a single update
- **No Legacy Support**: Old variable names (`BACKEND_PORT`, `COORDINATOR_PORT`) will not be supported - they are replaced entirely
- **Complete Replacement**: All references to legacy variable names must be replaced with canonical names (`RPC_PORT`, `HTTP_PORT`) in a single implementation
