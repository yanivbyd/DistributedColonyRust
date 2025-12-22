# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Session Initialization

**At the start of each new Claude Code session**, please:
1. Read `.cursor/rules/my-rules.mdc` for development guidelines and coding standards
2. Review the **Specification Workflow** section below (lines 178-184) and follow it strictly for all feature work
3. Familiarize yourself with the architecture and common patterns in this document

## Overview

DistributedColonyRust is a distributed cellular automaton simulator that simulates creature colonies across multiple backend nodes. The system partitions a 2D grid world into spatial shards distributed across multiple backend processes, with a central coordinator managing initialization, topology, and global events.

## Architecture

### Components (4 Rust crates in workspace)

**Backend** (`crates/backend`)
- Manages individual shards (250x250 grid partitions) of the colony
- Runs tick-based cellular automaton simulation (creatures eating, moving, breeding, killing)
- Exposes TCP RPC (port 8084) for coordinator commands and HTTP (port 8085) for GUI image fetches
- Exchanges border cells with adjacent shards after each tick for spatial consistency

**Coordinator** (`crates/coordinator`)
- Central orchestrator that initializes and manages the colony lifecycle
- Discovers backend nodes via ClusterRegistry (file-based localhost or AWS SSM)
- Distributes shards across backends and initializes ClusterTopology
- Broadcasts global events (creature creation, food changes, extinction, topography) to backends
- Exposes TCP RPC (port 8082) for backends and HTTP (port 8083) for control plane

**GUI** (`crates/gui`)
- egui-based visualization displaying creature colonies as shard images
- Fetches shard images and layer data (creatures, food, traits, health, age) from backends via HTTP
- Shows colony-wide statistics and historical events
- Supports both localhost (100ms refresh) and AWS modes (5000ms refresh)

**Shared** (`crates/shared`)
- Common types and protocols: `Cell`, `Shard`, `Traits`, `ColonyLifeRules`, `ClusterTopology`
- Communication APIs: `BackendRequest`/`BackendResponse`, `CoordinatorRequest`/`CoordinatorResponse`
- Service discovery abstraction via `ClusterRegistry` trait (file-based or AWS SSM Parameter Store)

### Communication Patterns

**TCP RPC**: Coordinator → Backend using bincode-serialized requests/responses with length-delimited framing over tokio TcpStream
- `InitColony`, `InitColonyShard`, `ApplyEvent`, `GetShardStats`, `StartTicking`

**HTTP**: GUI → Backend for image fetches, GUI → Coordinator for stats/events/topology

**Border Exchange**: After each tick, backends send `UpdatedShardContentsRequest` to adjacent shard hosts with border cell updates (top/bottom/left/right edges)

**Service Discovery**: Backends and coordinator register themselves in ClusterRegistry on startup; coordinator discovers available backends before initializing colony

### Deployment Modes

**Localhost**: All components on 127.0.0.1, file-based ClusterRegistry (`output/ssm/`), 25ms tick sleep, 4 backend instances on ports 8084-8091

**AWS**: Distributed across EC2 instances, AWS SSM Parameter Store for discovery, 100ms tick sleep, backends bind 0.0.0.0 but advertise private IPs

### Simulation Model

**Cell State**: Each cell contains: `tick_bit` (double-buffering), `food` (u16), `extra_food_per_tick` (u8), `color` (RGB), `health` (u16), `age` (u16), `traits` (size, can_kill, can_move)

**Tick Loop** (per shard):
1. Creatures eat food (limited by `eat_capacity_per_size_unit * size`)
2. Apply health costs based on size, can_kill, can_move traits
3. Move to neighbor with more food (if can_move)
4. Breed if health > threshold
5. Kill neighbors (if can_kill) or die from starvation
6. Add extra food per cell
7. Exchange borders with 8 adjacent shards
8. Increment tick counter

**Events**: Coordinator broadcasts `ColonyEvent` messages (CreateCreature, ChangeExtraFoodPerTick, Extinction, NewTopography, ChangeColonyRules) which backends apply during ticks

## Common Commands

### Build and Test

```bash
# Build all crates in release mode
cargo build --release

# Build specific crate
cargo build --release -p backend
cargo build --release -p coordinator
cargo build --release -p gui

# Run tests (all crates, with cloud feature for AWS integration)
cargo test --all --features cloud

# Run tests for specific crate
cargo test -p backend
cargo test -p shared
```

### Local Development

```bash
# Initial setup (macOS only - installs Homebrew, Rust)
./scripts/setup_mac.sh

# Run full local cluster (4 backends + coordinator + GUI)
./scripts/local_run.sh

# Kill all local processes
./scripts/local_kill.sh

# Start colony after nodes are running
./scripts/colony_start.sh
```

### AWS Deployment

```bash
# Full deployment cycle: destroy old stack, build Docker image, deploy CDK, test, launch GUI
./scripts/aws_full_cycle.sh

# SSH into coordinator or backend nodes
./scripts/ssh_coordinator.sh
./scripts/ssh_backend.sh

# Gather logs from all nodes
./scripts/gather_logs_from_nodes.sh

# Debug SSM parameters and network connectivity
# (runs automatically in aws_full_cycle.sh via /debug-ssm endpoint)
./scripts/debug_nodes.sh
```

### CDK Infrastructure

```bash
cd CDK

# Deploy AWS infrastructure
npm install
cdk deploy DistributedColonySpotInstances --require-approval never

# Destroy infrastructure
cdk destroy DistributedColonySpotInstances --force
```

### Running Individual Components

```bash
# Backend: <hostname> <rpc_port> <http_port> <mode>
cargo run --release -p backend -- 127.0.0.1 8084 8085 localhost

# Coordinator: <rpc_port> <http_port> <mode>
cargo run --release -p coordinator -- 8082 8083 localhost

# GUI: [mode]
cargo run --release -p gui         # localhost mode
cargo run --release -p gui aws     # AWS mode
```

## Development Guidelines (from .cursor/rules/my-rules.mdc)

### Code Quality
- Write simple, easy-to-read code; avoid repetition
- Keep changes minimal and aligned with the task
- Always build after changes and ensure zero errors or warnings
- High-level functions focus on business logic; extract low-level operations (TCP, serialization, file I/O) into helper utilities

### Rust Specifics
- Use `tokio` for async parallelism; never use `Rayon` (backend already uses it for shard ticking)
- Never clone large objects like `ColonyShard`
- Use `log!` for logging (not `crate::log!`)
- Prefer `.expect(...)` over custom logging macros
- Import commonly used types instead of fully qualified paths
- Always use `shared::utils::new_random_generator()` for RNG creation (never `SmallRng::from_entropy()` directly)
- Pass RNG objects to helper functions; avoid unnecessary re-creation
- Resolve all compiler warnings
- Include the **region** parameter when logging colony events in backend code
- Do not use `[COORD]` or `[BE]` prefixes in logs

### Architecture Patterns
- Keep `main.rs` files minimal; move logic into appropriate modules
- Consolidate duplicate constants or logic into the `shared` crate
- Avoid global counters or global mutable state
- Functions should read like clean API calls rather than exposing implementation details

### Specification Workflow
- Each spec begins with **Clarifications** section containing open questions
- Spec status: **waiting answers** → **draft** → **approved** (requires explicit Human Author approval)
- Implementation begins **only** when Human Author explicitly instructs it
- Specs limited to **150 lines or fewer** unless permitted otherwise
- Before creating a fix, try to create a failing unit test (unless too invasive)
- Never invent or assume APIs, functions, modules, configurations not explicitly confirmed

## Key Files and Locations

- **Main entry points**: `crates/backend/src/be_main.rs`, `crates/coordinator/src/coordinator_main.rs`, `crates/gui/src/gui_main.rs`
- **Communication protocols**: `crates/shared/src/be_api.rs`, `crates/shared/src/coordinator_api.rs`
- **Core simulation**: `crates/backend/src/colony_shard.rs`, `crates/shared/src/colony_model.rs`
- **Service discovery**: `crates/shared/src/cluster_registry.rs` (trait), implementations in backend/coordinator
- **Topology management**: `crates/shared/src/cluster_topology.rs`
- **Build profiles**: `Cargo.toml` defines `dev`, `release`, `fast`, `profiling` profiles
- **Output directories**: `output/logs/`, `output/ssm/` (local ClusterRegistry), `output/s3/` (snapshots), `output/run_logs/` (deployment logs)

## Port Allocations

**Localhost mode**:
- Coordinator: RPC=8082, HTTP=8083
- Backends: RPC=8084,8086,8088,8090; HTTP=8085,8087,8089,8091

**AWS mode**:
- Coordinator: RPC=8082, HTTP=8083 (override via RPC_PORT, HTTP_PORT env vars)
- Backends: Same as localhost (override via RPC_PORT, HTTP_PORT env vars)

## Important Patterns

### Double-Buffering
Cells use `tick_bit` to avoid read-write conflicts during parallel tick processing. After each tick, `tick_bit` is flipped to indicate which version is current.

### Border Exchange Protocol
After ticking, each shard exports its border cells (1-cell-wide strips on all 4 edges) and sends `UpdatedShardContentsRequest` to adjacent shard hosts. Adjacent shards integrate these updates using `tick_bit` to identify current generation.

### Idempotency
Coordinator's `/colony-start` endpoint requires `idempotency_key` query parameter to prevent duplicate initialization. Returns 202 (Accepted) on first call, 200 (OK) on subsequent calls with same key, 409 (Conflict) if already started with different key.

### Event Broadcasting
Coordinator ticker generates events at different frequencies (CreateCreature every N ticks, ChangeExtraFoodPerTick every M ticks, etc.) and broadcasts them to all backends via `ApplyEvent` RPC. Backends queue events and apply them during tick processing.

### Topology Initialization
1. Coordinator discovers available backends from ClusterRegistry
2. Creates shard map (distributes shards round-robin across backends)
3. Initializes ClusterTopology with shard-to-host mappings
4. Sends `InitColonyShard` RPC to each backend for assigned shards
5. Publishes topology to ClusterRegistry (file or SSM)
6. Sends `StartTicking` RPC to begin simulation

## Common Debugging

**Port conflicts**: Use `lsof -i :<port>` to check if ports are in use before starting local cluster

**Backend not discovered**: Check `output/ssm/` for registration files in localhost mode, or use `/debug-ssm` HTTP endpoint on coordinator in AWS mode

**Shard border misalignment**: Verify `UpdatedShardContentsRequest` messages are reaching adjacent backends (check logs for "Received updated shard contents from")

**Colony not starting**: Check coordinator logs for backend discovery issues; verify `idempotency_key` is provided in `/colony-start` POST request

**GUI shows blank/stale images**: Verify backends are ticking (check `/colony-info` endpoint), ensure HTTP ports are accessible from GUI

**AWS instance not accessible**: Verify security group allows inbound on HTTP/RPC ports; check SSM Parameter Store for correct IP registration
