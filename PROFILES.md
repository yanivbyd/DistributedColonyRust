# Cargo Profiles for DistributedColonyRust

This project provides multiple Cargo profiles optimized for different use cases.

## Available Profiles

### 🚀 `dev` (Default)
- **Optimization**: `opt-level = 0` (No optimization)
- **Use case**: Development and debugging
- **Pros**: Fastest compilation, full debug information
- **Cons**: Slowest runtime performance
- **Command**: `cargo run` (default)

### ⚡ `fast` (Quick Development)
- **Optimization**: `opt-level = 1` (Light optimization)
- **Use case**: Quick development iterations
- **Pros**: Fast compilation, moderate runtime performance
- **Cons**: Slower runtime than balanced/profiling
- **Command**: `cargo run --profile=fast`

### 🎯 `balanced` (Recommended for Development)
- **Optimization**: `opt-level = 3` (Maximum optimization)
- **Use case**: Development builds and runs with maximum performance
- **Pros**: Maximum runtime performance, good debug information
- **Cons**: Slower compilation than fast/dev
- **Command**: `cargo run --profile=balanced`
- **Scripts**: `build_and_run.sh` (automatically uses this profile)

### 🔥 `profiling` (Production Performance)
- **Optimization**: `opt-level = 3` (Maximum optimization)
- **Use case**: Performance testing and production
- **Pros**: Maximum runtime performance
- **Cons**: Slowest compilation
- **Command**: `cargo run --profile=profiling`
- **Scripts**: `run_profiler.sh`

## Performance Comparison

| Profile    | Compilation Speed | Runtime Speed | Debug Info | Use Case |
|------------|------------------|---------------|------------|----------|
| `dev`      | 🟢 Fastest       | 🔴 Slowest    | ✅ Full    | Debugging |
| `fast`     | 🟡 Fast          | 🟡 Moderate   | ✅ Full    | Quick iterations |
| `balanced` | 🟡 Moderate      | 🟢 Fastest    | ✅ Full    | Development ⭐ |
| `profiling`| 🔴 Slowest       | 🟢 Fastest    | ✅ Full    | Production |

## Quick Start

### For Daily Development (Maximum Performance):
```bash
# Use the optimized build_and_run script (uses balanced profile)
./build_and_run.sh

# Or manually with balanced profile
cargo run --profile=balanced -p backend &
cargo run --profile=balanced -p coordinator &
cargo run --profile=balanced -p gui
```

### For Quick Development Iterations:
```bash
# Use fast profile for quick builds
cargo run --profile=fast -p backend
cargo run --profile=fast -p coordinator
cargo run --profile=fast -p gui
```

### For Performance Testing:
```bash
# Use the profiler script
./run_profiler.sh

# Or manually with profiling profile
cargo run --profile=profiling -p backend &
cargo run --profile=profiling -p coordinator &
cargo run --profile=profiling -p gui
```

### For Debugging:
```bash
# Use default dev profile
cargo run -p backend
cargo run -p coordinator
cargo run -p gui
```

## Profile Configuration Details

### Fast Profile (Quick Development):
- `opt-level = 1`: Light optimization
- `incremental = true`: Faster rebuilds
- `codegen-units = 256`: Maximum parallel compilation
- `lto = false`: Faster compilation
- **Goal**: Fast compilation for quick iterations

### Balanced Profile (Development Performance):
- `opt-level = 3`: Maximum optimization
- `incremental = true`: Faster rebuilds
- `codegen-units = 16`: Balanced parallel compilation
- `lto = "thin"`: Link-time optimization for better performance
- **Goal**: Maximum runtime performance for development

### Profiling Profile:
- Inherits from `release` profile
- `opt-level = 3`: Maximum optimization
- `debug = true`: Keeps debug symbols for profiling

## Why This Profile Structure?

We now have three distinct profiles for different development needs:

1. **`fast`**: When you need to compile quickly and iterate fast
2. **`balanced`**: When you want maximum performance for development (default for build_and_run.sh)
3. **`profiling`**: When you need production-level performance for testing

The `balanced` profile gives you the same performance as `profiling` but with better debug information and incremental builds for development.

## Benchmarking

Run the profile comparison benchmark:
```bash
./benchmark_profiles.sh
```

This will show compilation times for each profile to help you choose the right one for your needs.
