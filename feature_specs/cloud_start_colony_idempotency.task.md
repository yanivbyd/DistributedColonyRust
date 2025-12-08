# Cloud-Start: Return Failure if Colony Already Started

## Problem
The `POST /cloud-start` endpoint in the coordinator currently only checks if a cloud-start operation is already in progress, but does not check if the colony has already been started. This allows multiple cloud-start attempts even when the colony is already running, which can cause issues. Additionally, the operation should be idempotent - calling it multiple times with the same idempotency_key should have the same effect as calling it once, allowing safe retries without side effects.

## Requirements
1. The `POST /cloud-start` endpoint should return a failure response if the colony is already started (unless using the same idempotency_key)
2. Cloud-start requests should include an idempotency_key that is stored in coordinator memory
3. If the same idempotency_key is used and the colony is already started, return success (idempotent behavior)
4. If a different idempotency_key is used and the colony is already started, return error
5. The cloud-start bash script should treat failure as an error and exit with a non-zero exit code

## Implementation Plan

### 1. Coordinator HTTP Server (`crates/coordinator/src/http_server.rs`)

**Current Behavior:**
- Checks if cloud-start is in progress using `CLOUD_START_IN_PROGRESS` mutex
- Returns 409 Conflict if in progress
- Returns 202 Accepted if not in progress and starts the process

**Changes Needed:**
- Remove `CLOUD_START_IN_PROGRESS` mutex check - not needed anymore
- Accept idempotency_key in POST request as query parameter
- Add `cloud_start_idempotency_key: Option<String>` field to `CoordinatorStoredInfo` to store the idempotency_key used for initialization
- Before starting cloud-start, check if the colony is already initialized by checking coordinator's internal state:
  1. Get `CoordinatorContext::get_instance().get_coord_stored_info()`
  2. Check the `status` field which is of type `ColonyStatus` enum:
     - `ColonyStatus::NotInitialized` - Colony is not initialized (cloud-start can proceed)
     - `ColonyStatus::TopographyInitialized` - Colony is initialized (check idempotency_key)
  3. If `status` is `ColonyStatus::TopographyInitialized`, the colony is already initialized
- If colony is already initialized:
  - Check if the provided idempotency_key matches the stored `cloud_start_idempotency_key` in `CoordinatorStoredInfo`
  - If idempotency_key matches, return HTTP 200 OK with message "Colony already started (idempotent)"
  - If idempotency_key doesn't match or is different, return HTTP 409 Conflict with message "Colony already started"
- If colony is not initialized, proceed with cloud-start
- When cloud-start completes successfully, store the idempotency_key in `CoordinatorStoredInfo.cloud_start_idempotency_key` and persist to disk

**Request Format:**
- Query parameter: `POST /cloud-start?idempotency_key=some-unique-key`
- idempotency_key is required - if not provided, return HTTP 400 Bad Request
- The idempotency_key must always be provided by the client - never generate it on the server side

**Response Codes:**
- HTTP 200 OK: Colony already started with matching idempotency_key
- HTTP 202 Accepted: Cloud-start initiated
- HTTP 409 Conflict: Colony already started with different idempotency_key
- HTTP 400 Bad Request: idempotency_key not provided (required, must come from client)

**ColonyStatus Enum States:**
The `ColonyStatus` enum (defined in `coordinator_storage.rs`) has the following states:
- `ColonyStatus::NotInitialized` - Colony has not been initialized yet
- `ColonyStatus::TopographyInitialized` - Colony has been initialized (topography and shards are set up)

**Note:** The coordinator tracks colony initialization status internally in `CoordinatorStoredInfo` (stored in `CoordinatorContext`). The colony is considered initialized if `status` is `ColonyStatus::TopographyInitialized`. The `cloud_start_idempotency_key` field in `CoordinatorStoredInfo` stores the idempotency_key that was used to initialize the colony, allowing idempotent checks across coordinator restarts.

**Note:** The check for colony initialization should be done synchronously before spawning the cloud-start task, so the HTTP response can immediately indicate failure or success.

### 2. Cloud-Start Bash Script (`scripts/cloud_start.sh`)

**Current Behavior:**
- Line 201-205: Treats HTTP 409 as a warning (not an error)
- Only exits with error if HTTP code is not 200-299 (except 409)

**Changes Needed:**
- Generate an idempotency_key based on timestamp - this must be done by the client script
- Include the idempotency_key in the POST request as query parameter: `POST /cloud-start?idempotency_key=<key>`
- Update error handling:
  - HTTP 200 OK with "idempotent" message: Treat as success (colony already started with same idempotency_key)
  - HTTP 202 Accepted: Treat as success (cloud-start initiated)
  - HTTP 409 Conflict: Treat as error and exit 1 (colony already started with different idempotency_key)
  - HTTP 400 Bad Request: Treat as error and exit 1 (invalid request)
  - Other non-2xx codes: Treat as error and exit 1

**Recommended Approach:**
- Generate idempotency_key in the bash script using: `date +%s` (Unix timestamp)
- Store idempotency_key in a variable and include in request
- The idempotency_key must be generated by the client (bash script), never by the server
- Check HTTP status code and response body
- Exit with error (exit 1) for any failure cases

### 4. Helper Functions and Data Structures

**Changes to `coordinator_storage.rs`:**
- Add `cloud_start_idempotency_key: Option<String>` field to `CoordinatorStoredInfo` struct
- Update `CoordinatorStoredInfo::new()` to initialize `cloud_start_idempotency_key` as `None`

**New Helper Functions in `http_server.rs`:**

1. **Check if colony is already started:**
```rust
fn is_colony_already_started() -> bool {
    // Check coordinator's internal state
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    
    // Colony is initialized if status is TopographyInitialized
    // ColonyStatus enum states:
    //   - NotInitialized: colony not initialized
    //   - TopographyInitialized: colony initialized
    matches!(stored_info.status, ColonyStatus::TopographyInitialized)
}
```

2. **Check if idempotency_key matches stored key:**
```rust
fn matches_stored_idempotency_key(key: &str) -> bool {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    
    // Check if the provided key matches the stored idempotency_key
    stored_info.cloud_start_idempotency_key.as_ref()
        .map(|stored_key| stored_key == key)
        .unwrap_or(false)
}
```

3. **Store idempotency_key after successful initialization:**
```rust
fn store_cloud_start_idempotency_key(key: String) {
    let context = CoordinatorContext::get_instance();
    let mut stored_info = context.get_coord_stored_info();
    stored_info.cloud_start_idempotency_key = Some(key);
    drop(stored_info); // Release lock before calling storage
    
    // Persist to disk
    let stored_info = context.get_coord_stored_info();
    if let Err(e) = CoordinatorStorage::store(&stored_info, COORDINATOR_STATE_FILE) {
        log_error!("Failed to save coordinator info with idempotency_key: {}", e);
    }
}
```

**Note:** The `is_colony_already_started()` function checks the coordinator's internal state, so it's synchronous and doesn't require any network calls or error handling. The idempotency_key is stored in `CoordinatorStoredInfo` which is persisted to disk, allowing idempotent checks to work across coordinator restarts.

## Files to Modify

1. `crates/coordinator/src/coordinator_storage.rs`
   - Add `cloud_start_idempotency_key: Option<String>` field to `CoordinatorStoredInfo` struct
   - Update `CoordinatorStoredInfo::new()` to initialize `cloud_start_idempotency_key` as `None`

2. `crates/coordinator/src/http_server.rs`
   - Remove `CLOUD_START_IN_PROGRESS` mutex (no longer needed)
   - Add `is_colony_already_started()` helper function (synchronous, checks CoordinatorContext)
   - Add `matches_stored_idempotency_key()` helper function to check if provided key matches stored key
   - Add `store_cloud_start_idempotency_key()` helper function to store key in CoordinatorStoredInfo and persist to disk
   - Update POST /cloud-start handler to:
     - Parse idempotency_key from request query parameter
     - If idempotency_key is not provided, return HTTP 400 Bad Request (idempotency_key is required and must come from client)
     - Check colony status before starting (using internal coordinator state)
     - If colony is initialized, check if provided idempotency_key matches stored `cloud_start_idempotency_key`
     - If matches, return HTTP 200 OK (idempotent)
     - If doesn't match, return HTTP 409 Conflict
     - If colony not initialized, proceed with cloud-start
     - When cloud-start completes successfully, call `store_cloud_start_idempotency_key()` to persist the key
   - Import necessary types (`CoordinatorContext`, `CoordinatorStorage`, `COORDINATOR_STATE_FILE`)

3. `scripts/cloud_start.sh`
   - Generate idempotency_key (must be generated by the client script, not the server)
   - Include idempotency_key in POST request as query parameter: `POST /cloud-start?idempotency_key=<key>`
   - Update error handling for all response codes
   - Treat HTTP 200 OK (idempotent) as success
   - Treat HTTP 409 Conflict as error and exit 1

4. `crates/coordinator/tests/test_cloud_start_colony.rs`
   - Add tests for colony already started scenario with idempotency_keys
   - Add tests for idempotent behavior

## Edge Cases to Consider

1. **CoordinatorContext not initialized**: If `CoordinatorContext::get_instance()` hasn't been initialized yet, `get_coord_stored_info()` will return default values (colony not initialized), which is correct behavior
2. **Race condition with same idempotency_key**: Multiple requests with same idempotency_key - first one starts, subsequent ones should check key status and return idempotent success if completed, or wait if in progress
3. **Race condition with different idempotency_keys**: Multiple requests with different idempotency_keys when colony already started - all should return 409 Conflict
4. **Idempotency_key not provided**: Return HTTP 400 Bad Request - the idempotency_key must always be provided by the client, never generated on the server
5. **Idempotency_key persistence**: The idempotency_key is stored in CoordinatorStoredInfo which is persisted to disk, so idempotent checks work across coordinator restarts
6. **Cloud-start fails after idempotency_key stored**: If cloud-start fails, the idempotency_key won't be stored (only stored on successful completion), so subsequent requests with same idempotency_key will start new cloud-start
