# GUI Public IP Support for AWS Mode

## Spec Header

**Spec Status**: approved

## Overview

This specification refactors `NodeAddress` to use `private_ip` and `public_ip` fields, making the intent explicit: internal communication between coordinator and backends uses private IP (for AWS VPC communication), while GUI uses public IP (for external access from outside AWS). Both coordinator and backends will store both IP addresses in SSM. The GUI will use the public IP when connecting from outside AWS (which is always the case, as GUI never runs on EC2).

**What this phase changes**:
- **NodeAddress structure**: Rename `ip` to `private_ip` (for internal communication), add `public_ip` (for GUI access)
- **Coordinator registration**: Coordinator in AWS mode will fetch and store both private and public IPs in SSM
- **Backend registration**: Backends in AWS mode will also fetch and store both private and public IPs in SSM
- **GUI discovery**: GUI will use `public_ip` for HTTP connections (public IP in AWS mode, same as private IP in localhost mode)

**What stays unchanged**:
- Localhost mode behavior (both RPC and HTTP use 127.0.0.1)
- Internal node-to-node RPC communication (still uses private IP)

## Current State

**NodeAddress** (`crates/shared/src/cluster_topology.rs`):
- Contains: `ip: String`, `internal_port: u16`, `http_port: u16`
- Only stores private IP address (used for both RPC and HTTP)

**Coordinator Registration** (`crates/coordinator/src/coordinator_main.rs:217-234`):
- In AWS mode: Gets private IP via `get_ec2_private_ip()`
- Registers in SSM with only private IP: `NodeAddress::new(coordinator_ip, rpc_port, http_port)`
- SSM value: `{"ip":"172.31.10.52","internal_port":8082,"http_port":8083}`

**Backend Registration** (`crates/backend/src/be_main.rs:464-502`):
- In AWS mode: Gets private IP via `get_ec2_private_ip()`
- Registers in SSM with only private IP
- Same limitation as coordinator

**GUI Discovery** (`crates/gui/src/gui_main.rs:1033-1041`):
- Retrieves `NodeAddress` from SSM
- Uses `coordinator_addr.ip` (private IP) to build HTTP URL
- Fails when running from local machine because private IP is unreachable

**Problem**: GUI running on local machine cannot reach coordinator's or backends' private IPs (e.g., 172.31.10.52) because they're only accessible within AWS VPC. GUI needs public IPs for HTTP connections.

## Proposed Changes

### 1. Refactor NodeAddress Structure

Rename fields to make intent explicit:
```rust
pub struct NodeAddress {
    pub private_ip: String,   // Private IP for internal communication between coordinator and backends
    pub public_ip: String,   // Public IP for GUI access (same as private IP in localhost)
    pub internal_port: u16,  // RPC port (existing)
    pub http_port: u16,      // HTTP port (existing)
}
```

**Rationale**: Using separate fields makes it explicit that internal communication uses private IP (for VPC communication) while GUI uses public IP (for external access). In localhost mode, both fields will have the same value (127.0.0.1).

**Methods to update**:
- `NodeAddress::new(private_ip: String, public_ip: String, rpc_port: u16, http_port: u16)`: Takes both IPs explicitly
- `to_http_address()`: Returns `{public_ip}:{http_port}` (uses public IP in AWS mode)
- `to_internal_address()`: Returns `{private_ip}:{internal_port}` (always uses private IP)
- `to_address()`: Returns `{private_ip}:{internal_port}` (same as `to_internal_address()`)

### 2. Coordinator: Fetch and Store Both IPs

**In AWS mode** (`crates/coordinator/src/coordinator_main.rs`):
- Get private IP via `get_ec2_private_ip()` (for RPC communication)
- Get public IP via `get_ec2_public_ip()` from EC2 metadata service: `http://169.254.169.254/latest/meta-data/public-ipv4`
- If metadata service fails or returns empty, log error and fail registration (public IP is required for GUI access)
- Create `NodeAddress` with both IPs: `NodeAddress::new(private_ip, public_ip, rpc_port, http_port)`
- Register in SSM with both IPs

**In localhost mode**:
- Use same IP for both: `NodeAddress::new("127.0.0.1", "127.0.0.1", rpc_port, http_port)`

**SSM value after change**:
```json
{"private_ip":"172.31.10.52","public_ip":"3.252.213.4","internal_port":8082,"http_port":8083}
```

### 3. Backend: Fetch and Store Both IPs

**In AWS mode** (`crates/backend/src/be_main.rs:464-502`):
- Get private IP via `get_ec2_private_ip()` (for RPC communication)
- Get public IP via `get_ec2_public_ip()` from EC2 metadata service
- If metadata service fails or returns empty, log error and fail registration (public IP is required for GUI access)
- Create `NodeAddress` with both IPs: `NodeAddress::new(private_ip, public_ip, rpc_port, http_port)`
- Register in SSM with both IPs

**In localhost mode**:
- Use same IP for both: `NodeAddress::new("127.0.0.1", "127.0.0.1", rpc_port, http_port)`

### 4. GUI: Use Public IP for HTTP Connections

**In `retrieve_topology()`** (`crates/gui/src/gui_main.rs:1033-1041`):
- After discovering coordinator, use `public_ip` for HTTP connection: `http://{public_ip}:{http_port}/topology`
- This will be the public IP in AWS mode (accessible from local machine) or 127.0.0.1 in localhost mode

**In `retrieve_http_ports()` and backend communication** (`crates/gui/src/gui_main.rs:980-1026`):
- Use `public_ip` from discovered backend addresses for HTTP connections
- This ensures GUI can contact backends directly from local machine

**Rationale**: GUI always runs outside AWS, so it must use public IPs (stored in `public_ip`) for HTTP connections. The `public_ip` field makes this explicit and avoids confusion.

### 5. Add Utility Function for Public IP

**In `crates/shared/src/utils.rs`**:
- Add `get_ec2_public_ip()` function similar to `get_ec2_private_ip()`
- Query EC2 metadata service: `http://169.254.169.254/latest/meta-data/public-ipv4`
- Return `Option<String>` (None if not on EC2 or if request fails)
- Use same timeout and error handling pattern as `get_ec2_private_ip()`

## API Changes

**NodeAddress JSON format**:
- **AWS mode**: `{"private_ip":"172.31.10.52","public_ip":"3.252.213.4","internal_port":8082,"http_port":8083}`
- **Localhost format**: `{"private_ip":"127.0.0.1","public_ip":"127.0.0.1","internal_port":8082,"http_port":8083}`

**Behavior changes**:
- `to_http_address()`: Returns `{public_ip}:{http_port}` (uses public IP in AWS mode, 127.0.0.1 in localhost)
- `to_internal_address()`: Returns `{private_ip}:{internal_port}` (always uses private IP for internal communication)
- `to_address()`: Returns `{private_ip}:{internal_port}` (same as `to_internal_address()`)

## Impact Analysis

**Files Requiring Changes**:
1. `crates/shared/src/cluster_topology.rs`: Refactor `NodeAddress` to use `private_ip` and `public_ip`, update all methods
2. `crates/shared/src/utils.rs`: Add `get_ec2_public_ip()` function
3. `crates/coordinator/src/coordinator_main.rs`: Fetch both private and public IPs in AWS mode, create `NodeAddress` with both IPs
4. `crates/backend/src/be_main.rs`: Fetch both private and public IPs in AWS mode, create `NodeAddress` with both IPs
5. `crates/gui/src/gui_main.rs`: Use `public_ip` for HTTP connections in `retrieve_topology()` and backend communication
6. `crates/shared/src/cluster_registry.rs`: No changes needed (JSON serialization handles new field names automatically)
7. All other files that create or use `NodeAddress`: Update to use new field names

**Testing Requirements**:
- Test coordinator registration with both IPs in AWS mode
- Test backend registration with both IPs in AWS mode
- Test GUI connection using `public_ip` from local machine for coordinator
- Test GUI connection using `public_ip` from local machine for backends
- Test localhost mode (both IPs set to 127.0.0.1)
- Verify internal communication still uses `private_ip`
- Verify HTTP connections use `public_ip` (public IP in AWS, 127.0.0.1 in localhost)

## Implementation Steps

1. **Refactor `NodeAddress` structure**:
   - Rename `ip` to `private_ip`, add `public_ip` field
   - Update `NodeAddress::new()` to take both IPs explicitly
   - Update `to_http_address()` to use `public_ip`
   - Update all existing code that creates or uses `NodeAddress`

2. **Add utility function**:
   - Add `get_ec2_public_ip()` function to `crates/shared/src/utils.rs`

3. **Update coordinator and backend**:
   - Coordinator: Fetch both private and public IPs in AWS mode, create `NodeAddress` with both
   - Backend: Fetch both private and public IPs in AWS mode, create `NodeAddress` with both

4. **Update GUI**:
   - Modify `retrieve_topology()` to use `public_ip` for coordinator
   - Modify backend communication to use `public_ip` for backends

5. **Test**:
   - Test SSM entries contain both `private_ip` and `public_ip`
   - Test GUI connection from local machine to coordinator and backends

## Success Criteria

✅ `NodeAddress` uses `private_ip` and `public_ip` fields (replacing old `ip` field)
✅ Coordinator in AWS mode fetches and stores both private and public IPs in SSM
✅ Backend in AWS mode fetches and stores both private and public IPs in SSM
✅ GUI uses `public_ip` for HTTP connections to coordinator and backends
✅ Localhost mode works unchanged (both IPs set to 127.0.0.1)
✅ Internal communication uses `private_ip`
✅ HTTP connections use `public_ip` (public IP in AWS, 127.0.0.1 in localhost)
✅ GUI can connect to coordinator and backends from local machine in AWS mode
