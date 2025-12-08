# AWS Colony Initialization Panic Bug

## Problem
When deploying in AWS mode, the coordinator panics during cloud-start initialization with the error:
```
[ERROR] [PANIC] <no message> at crates/coordinator/src/coordinator_context.rs:24
```

After the panic, the coordinator repeatedly logs "Backend colony not initialized" and the colony fails to initialize.

## Root Cause
The panic occurs due to an initialization order issue in AWS deployment mode:

1. **In `coordinator_main.rs` (line 197)**: `coordinator_ticker::start_coordinator_ticker()` is called during startup, before the cloud-start HTTP request is received.

2. **In `coordinator_ticker.rs` (line 17)**: The ticker thread immediately calls `CoordinatorContext::get_instance()`, which initializes the `COORDINATOR_CONTEXT` OnceLock with default values via `get_or_init()`.

3. **Later, when cloud-start is triggered**: `cloud_start_colony()` calls `initialize_colony()`.

4. **In `init_colony.rs` (line 135)**: `initialize_colony()` calls `CoordinatorContext::initialize_with_stored_info(stored_info)` to initialize the context with stored information from disk.

5. **In `coordinator_context.rs` (line 24)**: `initialize_with_stored_info()` attempts to `.set()` the OnceLock, but it's already been initialized by the ticker, causing `.expect("CoordinatorContext should only be initialized once")` to panic.

## Evidence from Logs

### Coordinator Log (`coordinator_8083.log`)
```
[2025-12-08 11:11:52] Received cloud-start request via HTTP with idempotency_key: 1765192312 HTTP/1.1
[2025-12-08 11:11:52] Starting cloud-start process: discovering backends and creating shard map
[2025-12-08 11:11:52] Starting topology discovery from AWS SSM...
[2025-12-08 11:11:52] Discovered coordinator: Some(NodeInfo { node_type: Coordinator, address: NodeAddress { ip: "172.31.18.70", port: 8083 }, status: Active })
[2025-12-08 11:11:52] SSM: discovered 1 backend entries
[2025-12-08 11:11:52] Discovered 1 backends from SSM
[2025-12-08 11:11:52] Topology discovery complete: coordinator=true, backends=1
[2025-12-08 11:11:52] Found 1 available backend nodes
[2025-12-08 11:11:52]   - 172.31.21.91:8082
[2025-12-08 11:11:52]   172.31.21.91:8082: 40 shards
[2025-12-08 11:11:52] Created shard map with 40 shards distributed across 1 backends
[2025-12-08 11:11:52] ClusterTopology initialized with dynamic topology
[2025-12-08 11:11:52] No existing coordination info found, starting fresh
[2025-12-08 11:11:52] [ERROR] [PANIC] <no message> at crates/coordinator/src/coordinator_context.rs:24
[2025-12-08 11:11:52] Backend colony not initialized
```

The panic occurs immediately after "No existing coordination info found, starting fresh", which is logged in `init_colony.rs` line 131, right before the call to `CoordinatorContext::initialize_with_stored_info()` at line 135.

### Backend Log (`be_8082.log`)
The backend is running normally and accepting connections, but the coordinator never successfully initializes the colony, so the backend remains in an uninitialized state.

## Code Flow Analysis

### Initialization Sequence (Current - Broken)
1. `coordinator_main::main()` starts
2. `coordinator_ticker::start_coordinator_ticker()` is called (line 197)
3. Ticker thread spawns and calls `CoordinatorContext::get_instance()` → initializes with defaults
4. HTTP server starts, waits for cloud-start request
5. Cloud-start request received
6. `cloud_start_colony()` called
7. `initialize_colony()` called
8. `CoordinatorContext::initialize_with_stored_info()` called → **PANIC** (OnceLock already set)

### Expected Sequence (Fixed - Defensive Approach)
To avoid relying on correct initialization sequencing, the fix should use a **defensive programming approach**:

1. **The ticker should handle the case where context isn't initialized yet** - The ticker should gracefully handle the uninitialized state (which is expected in AWS mode before cloud-start) by checking if the context is available before accessing it, and skipping operations until initialization completes.

2. **`initialize_with_stored_info` should handle the case where it's already initialized** - The method should be idempotent, checking if the context is already initialized and either returning early (no-op) or updating the existing context with stored info if needed.

This approach makes the system more resilient and doesn't require careful sequencing of initialization calls.

**Note**: The colony itself must NOT be initialized until the cloud-start HTTP command is received. Only the `CoordinatorContext` structure should be initialized early to prevent the panic.

## AWS Mode Constraint
**Important**: In AWS deployment mode, the coordinator must **NOT** initialize the colony before receiving the cloud-start HTTP command. This is by design - the coordinator waits for the HTTP request to trigger initialization. The fix must respect this constraint and only initialize the `CoordinatorContext` structure itself (to avoid the panic), but must not trigger colony initialization until the HTTP command is received.

## Impact
- **Severity**: Critical - Colony cannot be initialized in AWS deployment mode
- **Affected Mode**: AWS deployment mode only (localhost mode works because initialization happens immediately)
- **User Impact**: Complete failure of AWS deployments - colony never starts

## Proposed Solution (Recommended - Defensive Approach)

To make the system more resilient and avoid relying on correct initialization sequencing, implement **both** of the following changes:

### 1. Make Ticker Handle Uninitialized Context
The ticker should gracefully handle the case where the context isn't initialized yet (which is expected in AWS mode before cloud-start). This prevents the ticker from initializing the context with default values.

**Changes needed:**
- Modify `coordinator_ticker.rs` to check if context is initialized before accessing it
- Add a mechanism to skip ticker operations until initialization completes
- The ticker should check if `CoordinatorContext::get_instance()` has been initialized with stored info (not just default values)
- If context is not initialized, skip ticker operations (log a debug message if needed) and retry on next tick
- Ensure ticker doesn't trigger colony initialization - it should only read context state once initialized

**Implementation approach:**
- Add a method to `CoordinatorContext` to check if it's been initialized with stored info (vs. just default values)
- In `coordinator_ticker.rs`, check this before accessing context
- Skip operations if not initialized, retry on next tick interval

### 2. Make `initialize_with_stored_info` Idempotent
Allow `initialize_with_stored_info` to be called even if the context is already initialized. This prevents panics if the ticker initializes the context first.

**Changes needed:**
- Modify `coordinator_context.rs::initialize_with_stored_info()` to check if context is already initialized
- If already initialized:
  - Check if the existing context has the same stored info (compare key fields)
  - If different, log a warning but don't panic (or update if possible)
  - If same or compatible, return early (no-op)
- If not initialized, proceed with initialization as before
- Ensure this doesn't trigger colony initialization - it only manages the context structure

**Implementation approach:**
- Use `COORDINATOR_CONTEXT.try_get()` to check if already initialized
- If initialized, compare stored info and decide whether to update or return early
- If not initialized, use `.set()` as before

### Why This Approach is Better
- **No sequencing dependency**: Works correctly regardless of initialization order
- **More resilient**: Handles edge cases and race conditions gracefully
- **Easier to maintain**: Future code changes won't break if initialization order changes
- **Defensive programming**: Each component handles its own edge cases

### Alternative: Initialize Context Before Ticker (Less Robust)
While it's possible to initialize `CoordinatorContext` with stored info in `main()` before starting the ticker, this approach is less robust because it relies on correct sequencing. If the initialization order changes in the future, the bug could reoccur. The defensive approach above is preferred.

## Files Affected
1. `crates/coordinator/src/coordinator_main.rs` - Initialization order
2. `crates/coordinator/src/coordinator_context.rs` - Panic location (line 24)
3. `crates/coordinator/src/init_colony.rs` - Calls `initialize_with_stored_info` (line 135)
4. `crates/coordinator/src/coordinator_ticker.rs` - Accesses context early (line 17)

## Testing
After fix:
1. Deploy in AWS mode
2. Trigger cloud-start via HTTP endpoint
3. Verify no panic occurs
4. Verify colony initializes successfully
5. Verify "Backend colony not initialized" messages stop
6. Verify coordinator and backend logs show successful initialization

## Related Code References
- `coordinator_context.rs:21-24` - `initialize_with_stored_info()` method that panics
- `coordinator_context.rs:13-19` - `get_instance()` method that initializes with defaults
- `coordinator_main.rs:197` - Ticker startup
- `coordinator_main.rs:201-204` - AWS mode initialization logic
- `init_colony.rs:127-135` - Colony initialization that triggers the panic
