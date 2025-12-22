## Backend Tick Latency Logging

**Status**: approved  

### Goal

Add **per-backend tick latency monitoring** that periodically logs two average latencies over windows of 50 ticks:

- **Core tick latency** (cell-processing work only).
- **Full tick latency** (core work plus border exchange and event application),

without changing core simulation behavior.

---

### Scope

- **In scope**
  - Measure **duration of each backend tick** in wall-clock time.
  - Maintain rolling statistics over **fixed windows of 50 ticks**.
  - Emit a backend log line each time 50 ticks complete, then reset counters.
- **Out of scope**
  - Changes to coordinator or GUI.
  - External metrics integration (Prometheus, CloudWatch, etc.).
  - New configuration flags (unless explicitly requested later).

---

### High-Level Behavior

- Each backend node maintains in-memory **tick latency accumulators** for:
  - **Core tick latency** (just the core shard tick section).
  - **Full tick latency** (core section plus border exchange and event application).
- For each backend tick:
  - Capture `start_full` immediately before any per-tick work begins.
  - Capture `start_core` immediately before the core shard tick work.
  - Run the existing core shard tick work.
  - Capture `end_core` immediately after the core shard tick work completes.
  - Run remaining per-tick work (border exchange, event application, etc.).
  - Capture `end_full` after all per-tick work finishes.
  - Compute:
    - `core_latency = end_core - start_core`.
    - `full_latency = end_full - start_full`.
- Maintain for the current window:
  - `window_tick_count`: number of ticks in the window.
  - `window_total_core_latency_ms`: cumulative **core** latency in milliseconds (or microseconds).
  - `window_total_full_latency_ms`: cumulative **full** latency in milliseconds (or microseconds).
- When `window_tick_count` reaches **50**:
  - Compute:
    - `avg_core_latency_ms = window_total_core_latency_ms / 50`.
    - `avg_full_latency_ms = window_total_full_latency_ms / 50`.
  - Emit a **single log line** at `Info` level.
  - Reset `window_tick_count`, `window_total_core_latency_ms` and `window_total_full_latency_ms` to zero.

---

### Logging Requirements

- Use the existing **`log!` macro** in the backend crate.
- Do **not** include `[BE]` or `[COORD]` prefixes.
- Log level: **Info**.
- Proposed log message (conceptual shape, exact text can be tuned at implementation time):
  - `"Shard tick latency window complete: ticks=50, avg_core_ms=<avg_core_latency_ms>, avg_full_ms=<avg_full_latency_ms>, shards=<shard_count>"`
- Frequency:
  - **Exactly once per 50 ticks per backend**.
  - No per-tick latency logs to avoid log spam.

---

### Data Structures & Placement

- Introduce a lightweight struct, e.g. `ShardTickLatencyStats`, in the backend crate:
  - `window_tick_count: u32`
  - `window_total_core_latency_ms: f64`
  - `window_total_full_latency_ms: f64`
- Integrate this struct into the **backend tick loop state**:
  - Attach to the existing tick runner used in both localhost and AWS modes.
  - Avoid any **global static state** or cloning of large objects like `ColonyShard`.
- Use `std::time::Instant` for timing:
  - Use separate `Instant` markers for **core** and **full** sections as described in High-Level Behavior.

---

### Algorithm (Per Backend)

1. **Initialization**
   - Construct `ShardTickLatencyStats` with zeroed fields when the backend tick loop starts.
2. **Per Tick**
   - Capture `start_full = Instant::now()` immediately before any per-tick work begins.
   - Capture `start_core = Instant::now()` immediately before the core shard tick section.
   - Run the existing core tick logic (no behavioral changes).
   - Capture `end_core = Instant::now()` immediately after the core tick section.
   - Run any remaining per-tick work (border exchange, event application, etc.).
   - Capture `end_full = Instant::now()` after all per-tick work finishes.
   - Compute:
     - `core_latency_ms` from `end_core - start_core`.
     - `full_latency_ms` from `end_full - start_full`.
   - Increment `window_tick_count` by 1.
   - Add `core_latency_ms` to `window_total_core_latency_ms`.
   - Add `full_latency_ms` to `window_total_full_latency_ms`.
3. **Window Check**
   - If `window_tick_count == 50`:
     - Compute `avg_core_latency_ms` and `avg_full_latency_ms`.
     - Determine the current **number of shards** hosted on the backend (using existing backend state).
     - Emit the log line via `log!` including `ticks`, `avg_core_ms`, `avg_full_ms`, and `shards`.
     - Reset `window_tick_count`, `window_total_core_latency_ms` and `window_total_full_latency_ms` to 0.

---

### Edge Cases

- **Early shutdown or stop**:
  - Do **not** log partial windows; only log full 50-tick aggregates when `window_tick_count == 50`.
- **Very long ticks**:
  - Use a sufficiently wide type (e.g., 64-bit microseconds or `f64` ms) to avoid overflow.
- **Mode consistency**:
  - Ensure identical behavior in **localhost** and **AWS** modes by reusing the same tick loop instrumentation.


