# GUI HTTP API Migration Specification

## Acknowledgment
Status: **approved**

## Overview

This specification defines the migration of the remaining GUI → Backend binary endpoints from RPC to HTTP APIs. This completes the migration of all GUI communication to HTTP, making the system more cloud-friendly, debuggable, and maintainable.

**What this changes**:
- GUI → Backend binary endpoints (`GetShardImage`, `GetShardLayer`) move from RPC to HTTP
- Backend HTTP server gains new binary endpoints using bincode serialization

**What stays unchanged**:
- Coordinator ↔ Backend internal communication (remains RPC)
- Core service logic and data structures
- Already migrated endpoints (colony-events, colony-stats, colony-info)

## Current State

**Still using RPC** (to be migrated):
- GUI → Backend: `GetShardImage`, `GetShardLayer` via `send_request_with_pool()` in `call_be.rs`

**Already HTTP** (not part of this migration):
- GUI → Coordinator: `GET /api/colony-events`, `POST /api/colony-stats`
- GUI → Backend: `GET /api/colony-info`
- `GET /topology` (coordinator), `POST /colony-start` (coordinator)

**Remains RPC** (internal communication only):
- Coordinator ↔ Backend internal communication

## Proposed Changes

### Shard ID Format

**Standard Shard ID Format**: `{x}_{y}_{width}_{height}` (e.g., `"0_0_500_500"`)

Use `Shard::to_id()` and `Shard::from_id()` to convert between `Shard` structs and shard_id strings.

### Backend HTTP API

**GET /api/shard/{shard_id}/image**
- Path parameter: `shard_id` in format `{x}_{y}_{width}_{height}` (e.g., `/api/shard/0_0_500_500/image`)
- Response (200): Binary data (Content-Type: `application/octet-stream`) - Raw RGB bytes: `width * height * 3` bytes, row-major order (each pixel is 3 consecutive bytes: R, G, B)
- Error (404): `{"error": "Shard not available"}`

**GET /api/shard/{shard_id}/layer/{layer_name}**
- Path parameters: `shard_id` (format above) and `layer_name` (kebab-case)
- Layer names: `creature-size`, `extra-food`, `can-kill`, `can-move`, `cost-per-turn`, `food`, `health`, `age`
- Response (200): Binary data (Content-Type: `application/octet-stream`) - Length prefix (4 bytes, u32 little-endian) + i32 array (each i32 is 4 bytes, little-endian), row-major order
- Errors: 400 (invalid shard_id format or layer name), 404 (shard not available)

### Design Principles

- Binary formats (both use `application/octet-stream`):
  - Shard image: Raw RGB bytes - `width * height * 3` bytes, row-major order (each pixel: R, G, B as u8)
  - Shard layer: Length-prefixed i32 array - 4 bytes (u32 LE) for count, followed by `count * 4` bytes (i32 LE each), row-major order
- Error format: `{"error": "description"}`
- Status codes:
  - 200 (OK): Successful response
  - 400 (Bad Request): Invalid request parameters (invalid metric, invalid shard_id format, invalid layer name)
  - 404 (Not Found): Resource not available (colony not initialized, shard not available)
  - 500 (Internal Error): Server error
- Reuse existing RPC handler logic, convert to standard binary formats (raw RGB bytes for images, length-prefixed little-endian i32 array for layers)

## Implementation Details

**Shared** (`shared/src/colony_model.rs`): Use existing `Shard::to_id()` and `Shard::from_id()` methods (already implemented).

**Backend** (`http_server.rs`): Add endpoints with URL path parsing. Use `Shard::from_id()` to convert shard_id to `Shard` struct. Handle `Result<Shard, String>` return type (return 400 Bad Request on parse errors). Parse kebab-case layer names and convert to `ShardLayer` enum. Reuse existing RPC handler logic (`ShardUtils::get_shard_image()`, `ShardUtils::get_shard_layer()`). Convert responses to standard binary formats:
  - Image: Convert `Vec<Color>` to raw RGB bytes (flatten to `[u8; width*height*3]`)
  - Layer: Write length (u32 LE) + i32 values (LE)

**GUI** (`call_be.rs`): Use `Shard::to_id()` to construct URLs. Replace `send_request_with_pool()` calls for `GetShardImage` and `GetShardLayer` with HTTP client calls using `reqwest::blocking::Client`. Use HTTP ports from topology/SSM discovery. Parse standard binary formats:
  - Image: Read raw RGB bytes and convert to `Vec<Color>`
  - Layer: Read length (u32 LE) + i32 values (LE)
Keep same return types (`Option<T>`). Use same timeouts as RPC (1500ms total).

## Implementation Tasks

1. **Add binary HTTP endpoints in backend**:
   - `GET /api/shard/{shard_id}/image` (returns bincode-serialized `Vec<Color>`)
   - `GET /api/shard/{shard_id}/layer/{layer_name}` (returns bincode-serialized `Vec<i32>`)
   - Both use `application/octet-stream` Content-Type with bincode serialization
   - Reuse existing RPC handler logic from `ShardUtils`

2. **Update GUI to use HTTP endpoints**:
   - Replace `send_request_with_pool()` calls for `GetShardImage` and `GetShardLayer` with HTTP client calls
   - Deserialize bincode responses to `Vec<Color>` and `Vec<i32>` respectively
   - Use `Shard::to_id()` to construct URLs

3. **Remove unused RPC code**:
   - Delete `GetShardImage`, `GetShardLayer` from `be_api.rs`
   - Remove RPC handler code (`handle_get_shard_image`, `handle_get_shard_layer`) from `be_main.rs`
   - Remove RPC client code from `gui/src/call_be.rs` for these endpoints

## Impact Analysis

**Files to modify**: 
- `backend/src/http_server.rs`: Add binary HTTP endpoints
- `gui/src/call_be.rs`: Replace RPC calls with HTTP client calls
- `backend/src/be_main.rs`: Remove RPC handler code
- `shared/src/be_api.rs`: Remove RPC request/response types

**Files that already have what we need**:
- `shared/src/colony_model.rs`: Shard ID helper methods (`Shard::to_id()` and `Shard::from_id()`)
- `backend/src/shard_utils.rs`: Handler logic (`ShardUtils::get_shard_image()`, `ShardUtils::get_shard_layer()`)

**Dependencies**: Existing `reqwest` (already in use)
