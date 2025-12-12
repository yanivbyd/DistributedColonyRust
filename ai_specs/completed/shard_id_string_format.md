# Shard ID String Format Specification

## Acknowledgment
Status: **approved**

## Overview

This specification defines a standard string format for identifying shards: `{x}_{y}_{width}_{height}`. This format will be used consistently throughout the application for shard identification in HTTP URLs, logging, and other contexts where a string representation is needed.

**What this phase changes**:
- Adds `Shard::to_id()` and `Shard::from_id()` methods to `Shard` impl in `shared/src/colony_model.rs`
- Moves/consolidates existing `shard_id()` logic from `backend/src/shard_utils.rs` to use `Shard::to_id()`
- Establishes standard shard_id format: `{x}_{y}_{width}_{height}` (e.g., `"0_0_500_500"`)
- Updates all existing code to use the new shard_id methods immediately

**What stays unchanged**:
- `Shard` struct definition
- Existing RPC communication (uses `Shard` struct directly)
- Internal data structures

## Current State

**Shard Identification**:
- `Shard` struct is defined in `shared/src/colony_model.rs` with fields: `x: i32, y: i32, width: i32, height: i32`
- Shards are used directly as structs throughout the codebase
- A `shard_id()` function exists in `backend/src/shard_utils.rs` that formats shards as `"{x}_{y}_{width}_{height}"` but it's not in the shared crate
- No standardized parsing function exists to convert shard_id strings back to `Shard`
- HTTP API migration will need shard_id strings in URLs and requires both formatting and parsing

## Proposed Changes

### Shard ID Format

**Format**: `{x}_{y}_{width}_{height}` where coordinates are integers separated by underscores.

**Examples**:
- `"0_0_500_500"` for shard at (0,0) with dimensions 500x500
- `"500_0_500_500"` for shard at (500,0) with dimensions 500x500
- `"-100_-200_300_400"` for negative coordinates (if supported)

### Helper Methods

**`Shard::to_id(&self) -> String`**
- Converts `Shard` struct to shard_id string
- Returns format: `"{x}_{y}_{width}_{height}"`
- No error cases (always succeeds)

**`Shard::from_id(id: &str) -> Result<Shard, String>`**
- Parses shard_id string to `Shard` struct
- Returns `Ok(Shard)` if format is valid
- Returns `Err(String)` with error message if format is invalid (wrong number of parts, non-integer values, etc.)
- Handles negative coordinates

### Design Principles

- Format is simple and URL-friendly (no special characters except underscore)
- Format is human-readable and debuggable
- Parsing returns descriptive error messages for invalid input (no panics)
- Methods are pure (no side effects)

## Implementation Details

**Location**: Add methods to `Shard` impl in `shared/src/colony_model.rs`

**Implementation**:
```rust
impl Shard {
    pub fn to_id(&self) -> String {
        format!("{}_{}_{}_{}", self.x, self.y, self.width, self.height)
    }
    
    pub fn from_id(id: &str) -> Result<Self, String> {
        let parts: Vec<&str> = id.split('_').collect();
        if parts.len() != 4 {
            return Err(format!("Invalid shard_id format: expected 4 parts separated by '_', got {}", parts.len()));
        }
        let x = parts[0].parse::<i32>()
            .map_err(|e| format!("Invalid x coordinate '{}': {}", parts[0], e))?;
        let y = parts[1].parse::<i32>()
            .map_err(|e| format!("Invalid y coordinate '{}': {}", parts[1], e))?;
        let width = parts[2].parse::<i32>()
            .map_err(|e| format!("Invalid width '{}': {}", parts[2], e))?;
        let height = parts[3].parse::<i32>()
            .map_err(|e| format!("Invalid height '{}': {}", parts[3], e))?;
        Ok(Shard { x, y, width, height })
    }
}
```

**Migration**: Update `backend/src/shard_utils.rs::shard_id()` to call `shard.to_id()` instead of implementing format directly.

## Migration Plan

**Phase 1**: Add `Shard::to_id()` and `Shard::from_id()` methods to `shared/src/colony_model.rs`  
**Phase 2**: Update `backend/src/shard_utils.rs::shard_id()` to use `shard.to_id()`  
**Phase 3**: Search codebase for other places that format shards as strings and update to use `Shard::to_id()`  
**Phase 4**: Use shard_id format in HTTP API endpoints (during HTTP API migration)

## Impact Analysis

**Files**: 
- `shared/src/colony_model.rs` (or new module): Add helper functions
- `backend/src/shard_utils.rs`: Update to use shared functions (remove duplicate `shard_id()`)

**Dependencies**: None (pure string manipulation)

**Testing**: Unit tests for `Shard::to_id()` and `Shard::from_id()` with various shard configurations, including edge cases (negative coordinates, large numbers, invalid strings). Test round-trip: `shard.to_id()` then `Shard::from_id(&id)` should return `Ok(shard)` for valid inputs. Test error messages for invalid formats.
