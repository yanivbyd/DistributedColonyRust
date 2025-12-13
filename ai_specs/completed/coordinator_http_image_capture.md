# Coordinator HTTP-Based Creature Image Capture

## Spec Header

**Status**: approved

---

## Overview

The coordinator should periodically capture creature images from all backends using the HTTP API (similar to how the GUI does it), combine them into a single image, and save it to disk. This feature works in both AWS and localhost deployment modes.

**What this changes**:
- Coordinator uses backend HTTP API (`GET /api/shard/{shard_id}/image`) instead of RPC
- Images saved to `output/s3/distributed_colony/images_shots/` directory (constant bucket name)
- Timestamp format: `YYYY_MM_DD__HH_MM_SS.png` (e.g., `2024_11_03__10_25_30.png`)

**What stays unchanged**:
- Image combination logic
- Periodic execution (every 60 seconds)
- Deployment mode support (AWS and localhost)

## Requirements

### Functional Requirements
1. **Periodic Execution**: Every minute, capture creature images from all backends
2. **HTTP API Usage**: Use backend HTTP API (`GET /api/shard/{shard_id}/image`) instead of RPC
3. **Deployment Modes**: Works in both AWS and localhost modes
4. **Image Construction**: Combine all shard images into a single unified image
5. **File Storage**: Save to `output/s3/distributed_colony/images_shots/` directory (constant bucket name)
6. **File Naming**: Use timestamp format `YYYY_MM_DD__HH_MM_SS.png` (double underscore between date and time, PNG format)

### Technical Requirements

#### Dependencies
- **image crate**: For image encoding/decoding and PNG format support (version 0.24)
- **chrono crate**: Already available in shared crate for timestamp formatting
- **reqwest crate**: For HTTP client (already available)

#### Module Structure
- **Update Module**: `colony_capture.rs` (or create new if doesn't exist)
  - `capture_colony()` - Main async function
  - `get_colony_dimensions()` - Helper to get colony dimensions
  - `get_shard_creature_image_http()` - Helper to fetch creature image via HTTP (similar to GUI)
  - `get_coordinator_timestamp()` - Helper to get coordinator wall clock timestamp for filename
  - `combine_shard_images()` - Helper to merge shard images into one
  - `save_image_to_disk()` - Helper to save PNG to disk with bucket directory

#### Integration Points
- **coordinator_main.rs**: 
  - Add/update module declaration: `mod colony_capture;`
  - Spawn a tokio task that runs every 60 seconds (in both AWS and localhost modes)
  - Task calls `colony_capture::capture_colony()`

#### File System
- **Directory**: `output/s3/distributed_colony/images_shots/`
- **Ensure directory exists**: Create directory if it doesn't exist before saving
- **File Format**: PNG
- **Naming**: Timestamp-based with double underscore: `YYYY_MM_DD__HH_MM_SS.png`

## High-Level Changes

### 1. Dependencies (coordinator/Cargo.toml)
- Add `image = "0.24"` dependency (if not already present)
- `reqwest` and `chrono` already available

### 2. Update/Create Module: colony_capture.rs
- Implement HTTP-based image capture from all backends (similar to `gui/src/call_be.rs`)
- Use constant bucket name: `distributed_colony/images_shots`
- Get coordinator wall clock timestamp for filename
- Combine shard images into single image
- Save PNG to `output/s3/distributed_colony/images_shots/` directory
- Use timestamp format with double underscore: `YYYY_MM_DD__HH_MM_SS.png`

### 3. coordinator_main.rs
- Add/update module declaration: `mod colony_capture;`
- Spawn periodic task (every 60 seconds) that runs in both deployment modes
- Task should be spawned after coordinator initialization

## Implementation Details

### Image Capture Flow
1. Get colony dimensions (width, height) from CoordinatorContext or backend
2. Get all shards from ClusterTopology
3. Get coordinator wall clock timestamp for filename
4. For each shard:
   - Get backend host from topology
   - Get HTTP port from topology/SSM discovery (similar to GUI)
   - Call `GET /api/shard/{shard_id}/image` using `reqwest` (blocking client, similar to GUI)
   - Parse raw RGB bytes response into `Vec<Color>`
5. Create combined image buffer (colony_width × colony_height)
6. For each shard image:
   - Calculate global position from shard coordinates (shard.x, shard.y)
   - Place pixels in correct position in combined image
7. Encode combined image as PNG using `image` crate
8. Create `output/s3/distributed_colony/images_shots/` directory if needed
9. Save PNG file with timestamp name: `YYYY_MM_DD__HH_MM_SS.png`

### HTTP API Usage (Similar to GUI)
- Use `reqwest::blocking::Client` with timeout (1500ms, same as GUI)
- Construct URL: `http://{hostname}:{http_port}/api/shard/{shard_id}/image`
- Parse response: Raw RGB bytes (`width * height * 3` bytes) into `Vec<Color>`
- Use `get_backend_http_port()` helper pattern from GUI (or similar SSM discovery)

### Bucket Name
- **Constant**: Use hardcoded bucket name `distributed_colony/images_shots`
- Works in both AWS and localhost modes
- No configuration needed

### Timestamp Format
- Format: `YYYY_MM_DD__HH_MM_SS.png` (double underscore between date and time)
- Use coordinator's current system time (wall clock) using `chrono::Local::now()`
- Example: `2024_11_03__10_25_30.png`

### Error Handling
- Log errors but don't crash if:
  - Backend HTTP request fails (log and continue with other shards)
  - Shard image retrieval fails (log and continue with other shards)
  - Image encoding fails (log error and return early)
  - File write fails (log error and return early)
- Continue with available shards even if some fail
- Use `log_error!` macro for error logging

### Performance Considerations
- Use blocking HTTP client (consistent with GUI pattern)
- Image combination is CPU-bound, runs in async context
- File I/O uses standard library (synchronous, acceptable for periodic task)
- Task runs every 60 seconds, so performance impact is minimal

### Code Style Guidelines
- Follow existing codebase patterns from `gui/src/call_be.rs` for HTTP calls
- Import `log!` and `log_error!` at the top (e.g., `use shared::{log, log_error};`)
- Use `log!` and `log_error!` for logging (not `shared::log!`)
- Keep functions focused and readable
- Use `Option` and `Result` types appropriately
- Handle errors gracefully without panicking

## Testing Considerations
- Verify images are saved correctly to `output/s3/distributed_colony/images_shots/`
- Verify timestamp format matches `YYYY_MM_DD__HH_MM_SS.png`
- Verify directory creation works
- Verify behavior in both AWS and localhost modes
- Verify periodic execution (every 60 seconds)
- Verify graceful handling of backend HTTP failures
- Verify image combination correctness (all shards in correct positions)

## Example Output
- Files saved as: `output/s3/distributed_colony/images_shots/2024_11_03__10_25_30.png`
- Log messages: "Starting creature image capture", "Collected N shard images", "Successfully saved creature image to: output/s3/distributed_colony/images_shots/YYYY_MM_DD__HH_MM_SS.png"

