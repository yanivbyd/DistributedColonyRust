# Remove State Persistence

## Acknowledgment
Not yet acknowledged - specification pending review

## Overview
This feature removes all colony state persistence to disk in both localhost and AWS deployment modes. This includes removing both writing state to disk and reading state from disk on startup. When coordinator and backends restart, they will always start with a fresh colony state instead of loading previously saved state. State persistence will be added in the future to support high availability features.

## Requirements

### Functional Requirements
1. **No State Persistence**: Neither coordinator nor backends should save colony state to disk in any deployment mode
2. **Fresh Start on Restart**: When coordinator and backends restart, they should always start with a fresh colony state (no loading from disk)
3. **State Files Not Created**: State files should not be created in `output/storage/` directory
4. **Future High Availability**: State persistence will be added in the future to support high availability features

### Technical Requirements

#### Coordinator Changes
- **State Saving**: Remove saving coordinator state to `output/storage/colony.dat`
  - Location: `crates/coordinator/src/coordinator_context.rs`
  - Methods: `add_colony_event()` and `update_colony_rules()`
  - Remove calls to `CoordinatorStorage::store()`
  
- **State Loading**: Remove loading coordinator state from disk
  - Location: `crates/coordinator/src/init_colony.rs`
  - In `initialize_colony()`, always start with `CoordinatorStoredInfo::new()`
  - Remove calls to `CoordinatorStorage::retrieve()`

#### Backend Changes
- **Shard Saving**: Remove saving shard state to `output/storage/{shard_id}.dat`
  - Location: `crates/backend/src/shard_utils.rs`
  - Method: `store_shard()`
  - Remove calls to `ShardStorage::store_shard()` or make the method a no-op
  
- **Shard Loading**: Remove loading shard state from disk
  - Location: `crates/backend/src/shard_utils.rs`
  - Method: `create_colony_shard()`
  - Always randomize shards at start, never attempt to load from disk
  - Remove calls to `ShardStorage::retrieve_shard()`

## High-Level Changes

### 1. Coordinator Storage Removal
- Remove all disk write operations from `CoordinatorContext` methods
- Remove all disk read operations from `init_colony.rs`
- Ensure `init_colony.rs` always starts fresh

### 2. Backend Storage Removal
- Remove all disk write operations from `ShardUtils::store_shard()`
- Remove all disk read operations from shard initialization
- Ensure shards always start randomized

## Implementation Details

### Coordinator State Persistence

**Current Behavior:**
- Coordinator saves state to `output/storage/colony.dat` whenever:
  - Colony events are added (`add_colony_event()`)
  - Colony rules are updated (`update_colony_rules()`)
- Coordinator loads state from disk on startup in `initialize_colony()`

**New Behavior:**
- `add_colony_event()`: Remove `CoordinatorStorage::store()` call
- `update_colony_rules()`: Remove `CoordinatorStorage::store()` call
- `initialize_colony()`: Always use `CoordinatorStoredInfo::new()`, remove `CoordinatorStorage::retrieve()` call

**Implementation Approach:**
- Remove storage calls from `CoordinatorContext` methods
- Remove retrieval call from `initialize_colony()`
- No deployment mode checks needed since persistence is removed entirely

### Backend Shard Persistence

**Current Behavior:**
- Backends save shards to `output/storage/{shard_id}.dat` whenever:
  - Shard is updated in ticker (`be_ticker.rs`)
  - Shard is updated after events (`be_colony_events.rs`)
  - Shard topography is initialized (`shard_topography.rs`)
- Backends load shards from disk when creating shards in `ShardUtils::create_colony_shard()`

**New Behavior:**
- `ShardUtils::store_shard()`: Remove disk write entirely (make it a no-op or remove the call)
- `ShardUtils::create_colony_shard()`: Always randomize, remove `retrieve_shard()` call

**Implementation Approach:**
- Remove storage calls from `ShardUtils::store_shard()` or make the method a no-op
- Remove retrieval call from `ShardUtils::create_colony_shard()`
- No deployment mode checks needed since persistence is removed entirely

## Error Handling
- No error handling needed since disk operations are removed
- Existing error handling code for disk operations can be removed
- No log messages needed since operations are removed entirely

## Testing Considerations
- Verify that no state files are created in `output/storage/` in any deployment mode
- Verify that restarting coordinator/backends always starts fresh in both localhost and AWS modes
- Test that colony always starts from scratch after restart

## Files to Modify

### Coordinator
- `crates/coordinator/src/coordinator_context.rs` - Remove storage calls from `add_colony_event()` and `update_colony_rules()`
- `crates/coordinator/src/init_colony.rs` - Remove retrieval call, always use `CoordinatorStoredInfo::new()`

### Backend
- `crates/backend/src/shard_utils.rs` - Remove storage/loading calls from `store_shard()` and `create_colony_shard()`

## Notes
- This change ensures consistent behavior across all deployment modes - always starting fresh
- State persistence will be added in the future to support high availability features
- No migration needed - existing state files will simply be ignored
- Storage infrastructure code can remain in place for future use, but should not be called
