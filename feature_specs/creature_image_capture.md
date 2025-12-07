# Spec: Periodic Creature Image Capture

## Overview
The coordinator should periodically capture creature images from all backends, combine them into a single image, and save it to disk. This feature works in both AWS and localhost deployment modes to facilitate debugging.

## Requirements

### Functional Requirements
1. **Periodic Execution**: Every minute, the coordinator should capture colony information from all backends (creatures image)
2. **Deployment Modes**: Feature runs in both AWS and localhost modes
3. **Backend Communication**: Call all backends to retrieve creature image data for their shards
4. **Image Construction**: Combine all shard images into a single unified image
5. **File Storage**: Save the combined image to disk under `output/s3/creatures_images/` directory
6. **File Naming**: Use timestamp format `YYYY_MM_DD_HH_MM_SS.png` (e.g., `2025_12_07_11_04_42.png`)

### Technical Requirements

#### Dependencies
- **image crate**: For image encoding/decoding and PNG format support (version 0.24)
- **chrono crate**: Already available in shared crate for timestamp formatting

#### Module Structure
- **New Module**: `colony_capture.rs`
  - `capture_colony()` - Main async function
  - `get_colony_dimensions()` - Helper to get colony dimensions from CoordinatorContext
  - `get_shard_creature_image()` - Helper to fetch creature image from a backend
  - `combine_shard_images()` - Helper to merge shard images into one
  - `save_image_to_disk()` - Helper to save PNG to disk

#### Integration Points
- **coordinator_main.rs**: 
  - Add module declaration: `mod colony_capture;`
  - Spawn a tokio task that runs every 60 seconds (in both AWS and localhost modes)
  - Task calls `colony_capture::capture_colony()`

#### File System
- **Directory**: `output/s3/creatures_images/`
- **Ensure directory exists**: Create directory if it doesn't exist before saving
- **File Format**: PNG
- **Naming**: Timestamp-based (e.g., `2025_12_07_11_04_42.png`)

## High-Level Changes

### 1. Dependencies (coordinator/Cargo.toml)
- Add `image = "0.24"` dependency

### 2. New Module: colony_capture.rs
- Implement image capture from all backends
- Combine shard images into single image
- Save PNG to `output/s3/creatures_images/` directory
- Create directory if it doesn't exist
- Use timestamp-based filename

### 3. coordinator_main.rs
- Add module declaration: `mod colony_capture;`
- Spawn periodic task (every 60 seconds) that runs in both deployment modes
- Task should be spawned after coordinator initialization

## Implementation Details

### Image Capture Flow
1. Get colony dimensions (width, height) from CoordinatorContext
   - Access via `CoordinatorContext::get_instance().get_coord_stored_info()`
   - If dimensions are `None`, log error and return early (colony not initialized)
2. Get all shards from ClusterTopology
3. For each shard:
   - Get backend host from topology
   - Connect to backend via TCP (blocking, consistent with `backend_client.rs` pattern)
   - Send `GetShardImageRequest`
   - Receive `GetShardImageResponse` with `Vec<Color>`
4. Create combined image buffer (colony_width × colony_height)
5. For each shard image:
   - Calculate global position from shard coordinates (shard.x, shard.y)
   - Place pixels in correct position in combined image
6. Encode combined image as PNG using `image` crate
7. Create `output/s3/creatures_images/` directory if needed (using `std::fs::create_dir_all`)
8. Save PNG file with timestamp name

### Error Handling
- Log errors but don't crash if:
  - Backend connection fails (log and continue with other shards)
  - Shard image retrieval fails (log and continue with other shards)
  - Image encoding fails (log error and return early)
  - File write fails (log error and return early)
- Continue with available shards even if some fail
- Use `log_error!` macro for error logging (consistent with codebase style)

### Performance Considerations
- Use blocking TCP connections (synchronous) for backend calls (consistent with existing `backend_client.rs` pattern)
- Image combination is CPU-bound, runs in async context
- File I/O uses standard library (synchronous, but acceptable for periodic task)
- Task runs every 60 seconds, so performance impact is minimal

### Code Style Guidelines
- Follow existing codebase patterns from `backend_client.rs`
- Import `log!` and `log_error!` at the top of the file in the use section (e.g., `use shared::{log, log_error};`)
- Use `log!` and `log_error!` for logging (not `shared::log!`)
- Keep functions focused and readable
- Use `Option` and `Result` types appropriately
- Handle errors gracefully without panicking

## Testing Considerations
- Verify images are saved correctly to `output/s3/creatures_images/`
- Verify timestamp format matches `YYYY_MM_DD_HH_MM_SS.png`
- Verify directory creation works
- Verify behavior in both AWS and localhost modes
- Verify periodic execution (every 60 seconds)
- Verify graceful handling of backend failures
- Verify image combination correctness (all shards in correct positions)

## Unit Tests

### Test File Structure
- **Test File**: `crates/coordinator/tests/test_colony_capture.rs`
- **Important**: Tests must be in separate files, NOT mixed with implementation code
- Do NOT add `#[cfg(test)]` mod blocks in the implementation file (`colony_capture.rs`)
- Test file should import from the implementation module: `use coordinator::colony_capture::*;`
- Tests should be organized by function being tested
- Use descriptive test names that explain what is being tested

### Test Functions to Implement

#### 1. `test_combine_shard_images()`
**Purpose**: Verify that shard images are correctly combined into a single image with proper positioning.

**Test Cases**:
- Single shard at origin (0, 0) - verify all pixels placed correctly
- Multiple shards in grid layout - verify each shard in correct position
- Shards with gaps between them - verify gaps remain black
- Overlapping shards (edge case) - verify later shard overwrites earlier
- Empty shard list - verify returns black image of correct size
- Shard extending beyond colony bounds - verify pixels are clipped

**Test Data**:
- Create mock shards with known positions and colors
- Use distinct colors per shard to verify positioning
- Test with various colony dimensions (small: 10x10, medium: 100x100, large: 1000x1000)

#### 2. `test_get_colony_dimensions()`
**Purpose**: Verify colony dimensions retrieval from CoordinatorContext.

**Test Cases**:
- Successful retrieval - verify correct width and height returned from CoordinatorContext
- Colony not initialized - verify returns None when dimensions are None
- Dimensions set correctly - verify dimensions match expected values after initialization

**Mocking Strategy**:
- Mock CoordinatorContext with test data
- Test with various colony dimensions
- Test with uninitialized colony (None values)

#### 3. `test_get_shard_creature_image()`
**Purpose**: Verify shard image retrieval from backend.

**Test Cases**:
- Successful retrieval - verify correct color vector returned
- Backend connection failure - verify returns None
- Shard not available response - verify returns None
- Invalid response format - verify returns None
- Empty color vector - verify returns Some(empty vec)

**Mocking Strategy**:
- Create mock TCP server that responds with `GetShardImageResponse`
- Test with various shard sizes
- Test with known color patterns to verify data integrity

#### 4. `test_save_image_to_disk()`
**Purpose**: Verify image is correctly saved to disk as PNG.

**Test Cases**:
- Successful save - verify file exists and is valid PNG
- Directory creation - verify directory is created if it doesn't exist
- File write failure - verify error handling (permissions, disk full)
- Invalid image data - verify error handling
- Concurrent writes - verify no file corruption (if applicable)

**Test Data**:
- Use temporary directory for test files
- Create test images with known dimensions and colors
- Verify PNG can be read back and matches original

#### 5. `test_timestamp_filename_format()`
**Purpose**: Verify timestamp-based filename generation.

**Test Cases**:
- Verify format matches `YYYY_MM_DD_HH_MM_SS.png`
- Verify no invalid characters in filename
- Verify uniqueness of filenames (within same second)
- Test with various timestamps (edge of day, month, year)

#### 6. `test_capture_colony_integration()`
**Purpose**: Integration test for the full capture flow.

**Test Cases**:
- Full successful flow - all shards retrieved and combined
- Partial failure - some shards fail, others succeed
- All shards fail - verify graceful error handling
- Empty shard list - verify early return
- Colony not initialized (dimensions are None) - verify early return with error log

**Mocking Strategy**:
- Mock multiple backend servers on different ports
- Simulate various failure scenarios
- Verify final image contains expected shard data

### Test Utilities and Helpers

#### Mock Backend Server
Create a helper function to spawn a mock TCP server that:
- Listens on a specified port
- Responds to `GetColonyInfoRequest` with configurable dimensions
- Responds to `GetShardImageRequest` with configurable color data
- Can simulate connection failures, timeouts, invalid responses

#### Test Image Helpers
- `create_test_image(width, height, color)` - Create test image with single color
- `create_test_shard_image(shard, colors)` - Create shard image with specific colors
- `assert_image_equals(image1, image2)` - Verify two images are identical
- `load_png_from_disk(path)` - Load and decode PNG for verification

#### Test Data Generators
- `generate_test_shards(count, width, height)` - Generate shard list with known positions
- `generate_test_colors(count)` - Generate distinct colors for testing
- `create_test_topology()` - Create ClusterTopology with test backends

### Test Organization

**File Structure:**
- Implementation: `crates/coordinator/src/colony_capture.rs` (NO tests here)
- Tests: `crates/coordinator/tests/test_colony_capture.rs` (ALL tests here)

**Test File Structure:**
```rust
use coordinator::colony_capture::*;

mod combine_shard_images_tests {
    // Tests for combine_shard_images()
}

mod get_colony_dimensions_tests {
    // Tests for get_colony_dimensions()
}

mod get_shard_creature_image_tests {
    // Tests for get_shard_creature_image()
}

mod save_image_to_disk_tests {
    // Tests for save_image_to_disk()
}

mod integration_tests {
    // Integration tests for full flow
}
```

### Test Execution
- Run tests with: `cargo test --package coordinator --test test_colony_capture`
- Tests should be fast (< 1 second each)
- Use temporary directories that are cleaned up after tests
- Mock external dependencies (TCP connections, file system) where possible
- Tests are in separate files, not in the implementation module

### Edge Cases to Test
- Zero-sized colony (width=0 or height=0)
- Very large colony (stress test)
- Shard with zero pixels
- Shard coordinates outside colony bounds
- Invalid PNG encoding
- File system errors (read-only directory, no space)
- Network timeouts and connection failures
- Concurrent image captures (if applicable)

## Example Output
- Files saved as: `output/s3/creatures_images/2025_12_07_11_04_42.png`
- Log messages: "Starting creature image capture", "Collected N shard images", "Successfully saved creature image to: output/s3/creatures_images/YYYY_MM_DD_HH_MM_SS.png"
