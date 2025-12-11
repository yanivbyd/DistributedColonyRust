# GUI Cluster Tab

## Spec Header

**Status**: draft

---

## Overview

This specification defines adding a new "Cluster" tab to the GUI that displays cluster deployment information including deployment mode, node roles, ports, and shard distribution across backends.

**Current State**:
- GUI has multiple tabs (Creatures, Extra Food, Food, Sizes, etc.) but no cluster overview
- GUI retrieves topology from coordinator via `GET /topology` at startup
- Topology contains coordinator host, backend hosts, and shard-to-host mapping
- GUI has access to deployment mode via command-line parameter

**Goal**:
- Add "Cluster" tab to GUI
- Display deployment mode (localhost vs. cloud/aws)
- List all nodes with their role (coordinator, backend)
- Show ports used by each node (RPC/internal port and HTTP port)
- For each backend, show the number of shards it contains

## Proposed Changes

### 1. Add Cluster Tab to GUI

**Tab Enum** (`crates/gui/src/gui_main.rs`):
- Add `Cluster` variant to `Tab` enum

**Tab UI**:
- Add "Cluster" selectable button in tab control
- Add `show_cluster_tab()` method to display cluster information

### 2. Cluster Tab Content

**Deployment Mode Display**:
- Show deployment mode (localhost or cloud/aws)
- Retrieve from GUI's mode parameter (passed at startup)

**Node List**:
- Display coordinator node:
  - Role: "Coordinator"
  - Hostname/IP
  - RPC port (internal port)
  - HTTP port
- Display each backend node:
  - Role: "Backend"
  - Hostname/IP
  - RPC port (internal port)
  - HTTP port
  - Shard count (number of shards assigned to this backend)

**Data Sources** (retrieved once at startup, no real-time updates):
- Topology from `GET /topology` (already retrieved at startup):
  - `coordinator_host: HostInfo` (hostname, internal port)
  - `backend_hosts: Vec<HostInfo>` (list of backend hosts with hostname, internal port)
  - `shard_to_host: HashMap<Shard, HostInfo>` (maps shards to hosts)
- HTTP ports: Retrieve from `ClusterRegistry` via `discover_coordinator()` and `discover_backends()` which return `NodeAddress` with `http_port` field (retrieved once at startup)
- Deployment mode: From GUI's command-line mode parameter (localhost or aws)
- Shard counts: Count shards in `shard_to_host` that map to each backend `HostInfo`

### 3. Implementation Details

**Shard Count Calculation**:
- For each backend in `backend_hosts`, count entries in `shard_to_host` where the value matches the backend's `HostInfo`
- Use `HostInfo` equality (hostname and port must match)

**HTTP Port Retrieval**:
- Use `ClusterRegistry` to discover coordinator and backends
- Extract `http_port` from `NodeAddress` returned by discovery methods
- Match nodes by comparing `HostInfo` (hostname + internal port) with `NodeAddress` (ip + internal_port)

**UI Layout**:
- Deployment mode: Display as header or prominent label
- Node list: Use table or list format with columns for Role, Hostname, RPC Port, HTTP Port, Shards (for backends)
- Format: Simple, readable layout consistent with other GUI tabs
- No interactive controls (no refresh button, no expandable details)
- Display only basic node information (role, hostname, ports, shard count)

## Impact Analysis

**Files Requiring Changes**:
- `crates/gui/src/gui_main.rs`:
  - Add `Cluster` to `Tab` enum
  - Add "Cluster" selectable button in tab control
  - Add `show_cluster_tab()` method
  - Store deployment mode in `BEImageApp` struct
  - Retrieve HTTP ports from `ClusterRegistry` (may need async runtime for discovery)
  - Calculate shard counts per backend from topology

**Considerations**:
- HTTP port retrieval requires `ClusterRegistry` access, which may need async runtime (done once at startup)
- Node matching between `HostInfo` (from topology) and `NodeAddress` (from registry) must handle hostname vs IP differences
- Shard count calculation is straightforward but must handle edge cases (empty topology, no shards)
- All data is retrieved and cached at GUI startup; no periodic updates or refresh mechanism

## Success Criteria

✅ Cluster tab appears in GUI tab list
✅ Deployment mode is displayed correctly (localhost or cloud/aws)
✅ Coordinator node is listed with role, hostname, RPC port, and HTTP port
✅ All backend nodes are listed with role, hostname, RPC port, HTTP port, and shard count
✅ Shard counts are accurate (match actual shard distribution)
✅ Tab displays correctly even when topology is empty or nodes have no shards
✅ UI is consistent with other GUI tabs
