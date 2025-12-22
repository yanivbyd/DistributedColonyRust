# Spec: Topology In-Progress Indication

**Status**: draft  
**Created**: 2025-12-22

## Overview

Currently, `GET /topology` returns `404 Not Found` when the topology is not initialized. This doesn't distinguish between:
- Topology not started (no colony-start request received)
- Topology initialization in-progress (colony-start request accepted, but topology not yet ready)

This spec adds an "in-progress" indication so clients can distinguish between these states and provide better user feedback.

## Current Implementation

- **Topology Endpoint**: `crates/coordinator/src/http_server.rs:446` - Returns 404 when topology not initialized
- **Colony Status**: `crates/coordinator/src/coordinator_storage.rs:9` - `ColonyStatus` enum with `NotInitialized` and `TopographyInitialized`
- **Colony Start**: `crates/coordinator/src/http_server.rs:93` - Returns 202 Accepted and spawns async task
- **GUI Auto-Init**: `crates/gui/src/gui_main.rs:1112` - Auto-initiates colony-start and polls with exponential backoff

## API Changes

### Modified Endpoint: `GET /topology`

**Current Behavior**:
- Returns `404 Not Found` with `{"error":"Topology not initialized"}` when topology is not available

**Proposed Behavior**:
- Returns `200 OK` with `{"status": "in-progress"}` when colony-start is in progress but topology not yet ready
- Returns `404 Not Found` with `{"error":"Topology not initialized"}` when no colony-start has been initiated
- Returns `200 OK` with topology data when topology is ready (existing behavior)

## Implementation Plan

### 1. Add Initializing Status

Add `ColonyStatus::Initializing` to the enum in `coordinator_storage.rs`:
- Set status to `Initializing` when colony-start begins (in HTTP handler before spawning async task)
- Status transitions: `NotInitialized` → `Initializing` → `TopographyInitialized`
- On failure, status reverts to `NotInitialized` (clears in-progress state)

### 2. HTTP Response Logic

Update `handle_get_topology()` to check status in order:
1. If topology is ready → return 200 OK with topology data (existing behavior)
2. If status is `Initializing` → return 200 OK with `{"status": "in-progress"}`
3. If status is `NotInitialized` → return 404 Not Found with `{"error":"Topology not initialized"}`

### 3. GUI Updates

Update GUI retry logic in `retrieve_topology()`:
- Handle 200 OK with `{"status": "in-progress"}` response
- Continue polling until status is no longer in-progress
- Maintain existing exponential backoff retry logic

## Files Changed

| File | Lines Changed | Description |
|------|---------------|-------------|
| `crates/coordinator/src/coordinator_storage.rs` | +1 | Add `Initializing` variant to `ColonyStatus` enum |
| `crates/coordinator/src/http_server.rs` | ~15 | Set `Initializing` status when colony-start begins; check status in `handle_get_topology()` |
| `crates/coordinator/src/colony_start.rs` | ~5 | Set status to `TopographyInitialized` on success; revert to `NotInitialized` on failure |
| `crates/gui/src/gui_main.rs` | ~10 | Handle `{"status": "in-progress"}` response in retry logic |

**Total**: 4 files, ~31 lines of code

## Design Decisions

- **Status-based detection**: Using `ColonyStatus::Initializing` provides clear state tracking and is easier to reason about than checking multiple fields
- **200 OK for in-progress**: Returns 200 OK (not 202 or 503) to indicate the endpoint is functioning correctly, just not ready yet
- **Simple response format**: Just `{"status": "in-progress"}` - no additional metadata needed for now
- **Automatic cleanup**: On failure, status reverts to `NotInitialized`, automatically clearing the in-progress state

