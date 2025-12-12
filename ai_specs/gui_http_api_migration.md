# GUI HTTP API Migration Specification

## Acknowledgment
Status: **approved**

## Overview

This specification defines the migration of all GUI communication from RPC (bincode over TCP) to HTTP APIs. The goal is to standardize all GUI-to-service communication on HTTP, making the system more cloud-friendly, debuggable, and maintainable.

**What this phase changes**:
- All GUI → Coordinator communication moves from RPC to HTTP REST APIs
- All GUI → Backend communication moves from RPC to HTTP REST APIs
- Coordinator and Backend HTTP servers gain new endpoints to support GUI operations

**What stays unchanged**:
- Coordinator ↔ Backend internal communication (remains RPC)
- Core service logic and data structures

## Current State

**GUI → Coordinator (RPC)**: `GetColonyEvents`, `GetColonyStats` via `send_coordinator_request()` in `call_be.rs`  
**GUI → Backend (RPC)**: `GetShardImage`, `GetShardLayer`, `GetColonyInfo` via `send_request_with_pool()` in `call_be.rs`  
**Already HTTP**: `GET /topology` (coordinator), `POST /colony-start` (coordinator)

## Proposed Changes

### Coordinator HTTP API

**GET /api/colony-events?limit={n}** (optional limit, default 30)
- Response (200): `{"events": [{"tick": u64, "event_type": String, "description": String}]}`
- Error (404): `{"error": "Colony not initialized"}`

**POST /api/colony-stats** (body: `{"metrics": [String]}`)
- Response (200): `{"tick_count": u64, "metrics": [{"metric": String, "avg": f64, "buckets": [...]}]}`
- Errors: 400 (invalid metric), 404 (colony not initialized)

### Shard ID Format

**Standard Shard ID Format**: `{x}_{y}_{width}_{height}` (e.g., `"0_0_500_500"`)

Use `Shard::to_id()` and `Shard::from_id()` to convert between `Shard` structs and shard_id strings.

### Backend HTTP API

**GET /api/shard/{shard_id}/image**
- Path parameter: `shard_id` in format `{x}_{y}_{width}_{height}` (e.g., `/api/shard/0_0_500_500/image`)
- Response (200): Binary image data (Content-Type: `image/bmp`) - Uncompressed BMP format for simplicity
- Error (404): `{"error": "Shard not available"}`

**GET /api/shard/{shard_id}/layer/{layer_name}**
- Path parameters: `shard_id` (format above) and `layer_name` (kebab-case)
- Layer names: `creature-size`, `extra-food`, `can-kill`, `can-move`, `cost-per-turn`, `food`, `health`, `age`
- Response (200): Binary image data (Content-Type: `image/bmp`) - Uncompressed BMP format for simplicity
- Errors: 400 (invalid shard_id format or layer name), 404 (shard not available)

**GET /api/colony-info**
- Response (200): `{"width": i32, "height": i32, "shards": [...], "colony_life_rules": {...}, "current_tick": u64}`
- Error (404): `{"error": "Colony not initialized"}`

### Design Principles

- All JSON encoding with `application/json` Content-Type (except binary image responses)
- Error format: `{"error": "description"}`
- Status codes:
  - 200 (OK): Successful response
  - 400 (Bad Request): Invalid request parameters (invalid metric, invalid shard_id format, invalid layer name)
  - 404 (Not Found): Resource not available (colony not initialized, shard not available)
  - 500 (Internal Error): Server error
- Reuse existing RPC handler logic, add JSON serialization layer

## Implementation Details

**Coordinator** (`http_server.rs`): Add endpoints calling existing RPC handler logic, wrap with JSON serialization. Use `CoordinatorContext` for data access.

**Shared** (`shared/src/colony_model.rs`): Use existing `Shard::to_id()` and `Shard::from_id()` methods (already implemented in `shared/src/colony_model.rs`).

**Backend** (`http_server.rs`): Add endpoints with URL path parsing. Use `Shard::from_id()` to convert shard_id to `Shard` struct. Handle `Result<Shard, String>` return type (return 400 Bad Request on parse errors). Parse kebab-case layer names and convert to `ShardLayer` enum. Reuse existing RPC handler logic.

**GUI** (`call_be.rs`): Use `Shard::to_id()` to construct URLs. Replace `send_coordinator_request()` and `send_request_with_pool()` with HTTP client calls using `reqwest::blocking::Client`. Use HTTP ports from topology/SSM discovery. Remove connection pooling. Keep same return types (`Option<T>`). Use same timeouts as RPC (500ms write, 1000ms read).

## Migration Plan

**Phase 1**: JSON HTTP APIs
- Add JSON HTTP endpoints:
  - Coordinator: `GET /api/colony-events`, `POST /api/colony-stats`
  - Backend: `GET /api/colony-info`
- Update GUI to use JSON HTTP APIs (replace `send_coordinator_request()` and `send_request_with_pool()` with HTTP client calls)
- Remove unused RPC code for JSON endpoints:
  - Delete `GetColonyEvents`, `GetColonyStats`, `GetColonyInfo` from `coordinator_api.rs` and `be_api.rs`
  - Remove RPC handler code for these endpoints from coordinator and backend
  - Remove RPC client code from `gui/src/call_be.rs` for these endpoints

**Phase 2**: Binary image HTTP APIs
- Add binary image endpoints:
  - Backend: `GET /api/shard/{shard_id}/image` (returns binary BMP, uncompressed)
  - Backend: `GET /api/shard/{shard_id}/layer/{layer_name}` (returns binary BMP, uncompressed)
- Update GUI to consume binary image responses
- Remove unused RPC code for binary endpoints:
  - Delete `GetShardImage`, `GetShardLayer` from `be_api.rs`
  - Remove RPC handler code for these endpoints from backend
  - Remove RPC client code from `gui/src/call_be.rs` for these endpoints

## Impact Analysis

**Files**: 
- `shared/src/colony_model.rs`: Shard ID helper methods already exist (`Shard::to_id()` and `Shard::from_id()`)
- `coordinator/src/http_server.rs`: Add HTTP endpoints
- `backend/src/http_server.rs`: Add HTTP endpoints  
- `gui/src/call_be.rs`: Replace RPC with HTTP calls

**Dependencies**: Existing `reqwest`, `serde_json` (already in use)
