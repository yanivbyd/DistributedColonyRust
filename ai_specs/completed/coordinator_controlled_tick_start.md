# Coordinator-Controlled Tick Start

## Spec Header

**Status**: approved

---

## Overview

Currently, backend tickers start automatically when backends start, and coordinator ticker starts immediately when coordinator starts. This causes issues:
- Backends tick before topology is initialized (spamming "Topology not initialized" errors)
- Coordinator ticker queries shards before they're initialized (causing "Shard not available" errors)
- No coordination between when ticks should actually start

This specification defines a new RPC mechanism where the coordinator explicitly controls when ticking begins across all backends, ensuring ticks only start after colony-start completes successfully.

**Note**: "colony-start" (formerly "cloud-start") refers to the process of initializing the colony topology and shards. The term "colony-start" is used throughout this spec for clarity.

## Problem Statement

**Current Behavior**:
- Backend tickers (`start_be_ticker()`) start automatically when backend starts
- Coordinator ticker (`start_coordinator_ticker()`) starts immediately when coordinator starts
- Both run continuously, querying/processing even when topology/shards aren't ready
- Results in:
  - Backend 8086 spamming "Topology not initialized, skipping tick" (2M+ log lines)
  - Coordinator logging "Shard not available on backend" repeatedly during initialization
  - Race conditions between initialization and ticking

**Root Cause**:
- No synchronization between initialization completion and tick start
- Tickers assume resources are ready when they're not

## Proposed Solution

### Architecture

1. **Backend Tickers**: Do NOT start automatically. Wait for explicit start command from coordinator.

2. **Coordinator Ticker**: Do NOT start automatically. Start only after colony-start completes successfully.

3. **New RPC**: `StartTicking` request from coordinator to backends to begin ticking.

4. **Timing**: Coordinator calls `StartTicking` on all backends only after:
   - Topology is initialized
   - All shards are initialized on their respective backends
   - Topography is initialized (if applicable)
   - Colony status is `TopographyInitialized`

## RPC Protocol

### New Backend Request

```rust
// In shared/src/be_api.rs
#[derive(Serialize, Deserialize, Debug)]
pub enum BackendRequest {
    // ... existing variants ...
    StartTicking(StartTickingRequest),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StartTickingRequest {
    // Empty for now, can be extended with parameters if needed
}
```

### New Backend Response

```rust
// In shared/src/be_api.rs
#[derive(Serialize, Deserialize, Debug)]
pub enum BackendResponse {
    // ... existing variants ...
    StartTicking(StartTickingResponse),
}

#[derive(Serialize, Deserialize, Debug)]
pub enum StartTickingResponse {
    Ok,
    ColonyNotInitialized,
    TopologyNotInitialized,
    Error(String),
}
```

## Implementation Details

### 1. Backend Changes

**File**: `crates/backend/src/be_main.rs`

**Changes**:
- Remove automatic `start_be_ticker()` call from backend startup
- Add handler for `BackendRequest::StartTicking`
- Handler should:
  - Verify colony is initialized
  - Verify topology is initialized
  - If both ready: call `start_be_ticker()` (only once)
  - Return appropriate response

**New Handler**:
```rust
async fn handle_start_ticking(_req: StartTickingRequest) -> BackendResponse {
    if !Colony::is_initialized() {
        return BackendResponse::StartTicking(StartTickingResponse::ColonyNotInitialized);
    }
    
    if ClusterTopology::get_instance().is_none() {
        return BackendResponse::StartTicking(StartTickingResponse::TopologyNotInitialized);
    }
    
    // Start ticking (idempotent - use OnceLock or similar to ensure only called once)
    start_be_ticker();
    
    BackendResponse::StartTicking(StartTickingResponse::Ok)
}
```

**Backend Startup**:
- Remove `start_be_ticker()` call from `main()` or startup code
- Backend should wait for `StartTicking` request before beginning ticks

### 2. Coordinator Changes

**File**: `crates/coordinator/src/init_colony.rs`

**Changes**:
- After successful colony initialization (all shards initialized, topography initialized, status set to `TopographyInitialized`):
  - Call new function `start_colony_ticking()` which:
    1. Starts coordinator ticker
    2. Sends `StartTicking` request to all backends

**New Function**:
```rust
pub async fn start_colony_ticking() {
    log!("Starting colony ticking: initiating coordinator ticker and notifying all backends");
    
    // Step 1: Start coordinator ticker
    coordinator_ticker::start_coordinator_ticker();
    
    // Step 2: Get topology and all backends
    let topology = match ClusterTopology::get_instance() {
        Some(t) => t,
        None => {
            log_error!("Cannot start ticking: topology not initialized");
            return;
        }
    };
    
    // Step 3: Send StartTicking to all unique backends
    let backend_hosts = topology.get_all_backend_hosts();
    let mut unique_backends: HashSet<HostInfo> = HashSet::new();
    for host in backend_hosts {
        unique_backends.insert(host);
    }
    
    for backend_host in unique_backends {
        match send_start_ticking_to_backend(&backend_host).await {
            Ok(StartTickingResponse::Ok) => {
                log!("Backend {}:{} started ticking", backend_host.hostname, backend_host.port);
            }
            Ok(StartTickingResponse::ColonyNotInitialized) => {
                log_error!("Backend {}:{} cannot start ticking: colony not initialized", 
                          backend_host.hostname, backend_host.port);
            }
            Ok(StartTickingResponse::TopologyNotInitialized) => {
                log_error!("Backend {}:{} cannot start ticking: topology not initialized", 
                          backend_host.hostname, backend_host.port);
            }
            Ok(StartTickingResponse::Error(msg)) => {
                log_error!("Backend {}:{} failed to start ticking: {}", 
                          backend_host.hostname, backend_host.port, msg);
            }
            Err(e) => {
                log_error!("Failed to send StartTicking to {}:{}: {}", 
                          backend_host.hostname, backend_host.port, e);
            }
        }
    }
    
    log!("Colony ticking started: coordinator ticker active, {} backends notified", unique_backends.len());
}

async fn send_start_ticking_to_backend(backend_host: &HostInfo) -> Result<StartTickingResponse, String> {
    let mut stream = connect_to_backend(&backend_host.hostname, backend_host.port).await
        .map_err(|e| format!("Connection failed: {}", e))?;
    
    let request = BackendRequest::StartTicking(StartTickingRequest {});
    send_message(&mut stream, &request).await;
    
    if let Some(response) = receive_message::<BackendResponse>(&mut stream).await {
        match response {
            BackendResponse::StartTicking(resp) => Ok(resp),
            _ => Err("Unexpected response type".to_string()),
        }
    } else {
        Err("Failed to receive response".to_string())
    }
}
```

**File**: `crates/coordinator/src/coordinator_main.rs`

**Changes**:
- Remove `coordinator_ticker::start_coordinator_ticker()` call from `main()`
- Coordinator ticker will be started by `start_colony_ticking()` after initialization

### 3. Timing and Sequence

**Colony-Start Flow** (formerly cloud-start):
1. `POST /colony-start` received (endpoint renamed from `/cloud-start`)
2. Discover backends
3. Create topology
4. Initialize topology
5. `initialize_colony()` called:
   - Initialize colony on all backends
   - Initialize all shards on their respective backends
   - Initialize topography
   - Set status to `TopographyInitialized`
6. **NEW**: Call `start_colony_ticking()`:
   - Start coordinator ticker
   - Send `StartTicking` to all backends
7. Colony-start completes

**Backend Flow**:
1. Backend starts
2. Backend ticker does NOT start automatically
3. Backend waits for `StartTicking` request
4. When received:
   - Verify colony and topology are ready
   - Start backend ticker
   - Return success

## Error Handling

### Backend Errors

- **ColonyNotInitialized**: Backend hasn't initialized colony yet (shouldn't happen if called after colony-start)
- **TopologyNotInitialized**: Backend hasn't received topology yet (shouldn't happen if called after colony-start)
- **Error(String)**: Other errors (e.g., ticker already started)

### Coordinator Errors

- If any backend fails to start ticking, log error but continue
- Coordinator ticker should still start even if some backends fail
- Consider retry mechanism for failed backends (future enhancement)

## Backward Compatibility

**Breaking Changes**:
- Backends will not tick automatically - requires coordinator to send `StartTicking`
- Coordinator ticker will not start automatically - requires explicit call after initialization

**Migration**:
- This is a breaking change for the tick initiation flow
- All deployments must use colony-start (formerly cloud-start, already required by dynamic topology elimination)
- HTTP endpoint `/cloud-start` should be renamed to `/colony-start` for clarity
- All references to "cloud-start" in code and documentation should be updated to "colony-start"

## Success Criteria

✅ Backend tickers do NOT start automatically on backend startup
✅ Coordinator ticker does NOT start automatically on coordinator startup  
✅ `StartTicking` RPC implemented and working
✅ Coordinator calls `StartTicking` on all backends after colony-start completes
✅ Backends verify colony and topology are ready before starting ticks
✅ No "Topology not initialized" spam in backend logs
✅ No "Shard not available" spam in coordinator logs during initialization
✅ Ticks only begin after all initialization is complete
✅ HTTP endpoint renamed from `/cloud-start` to `/colony-start`
✅ All code and documentation references updated from "cloud-start" to "colony-start"

## Files to Modify

1. `crates/shared/src/be_api.rs`: Add `StartTickingRequest` and `StartTickingResponse`
2. `crates/backend/src/be_main.rs`: 
   - Remove automatic `start_be_ticker()` call
   - Add `handle_start_ticking()` handler
3. `crates/coordinator/src/init_colony.rs`:
   - Add `start_colony_ticking()` function
   - Call it after successful initialization
4. `crates/coordinator/src/coordinator_main.rs`:
   - Remove `coordinator_ticker::start_coordinator_ticker()` call
5. `crates/backend/src/be_ticker.rs`:
   - Ensure `start_be_ticker()` is idempotent (can be called multiple times safely)

