## Spec Header

- **Spec name**: Add OriginalColor StatMetric
- **Spec status**: **approved**

---

## Clarifications (Answered)

1. **Color representation in StatBucket**: Option C - Create a separate structure for string-based metrics
2. **Histogram filtering**: Yes - follow same filtering rules (minimum 20 occurrences, top 20 values)
3. **Average calculation**: No average calculation for OriginalColor
---

## Main specification

### Goal

Add a new `OriginalColor` variant to the `StatMetric` enum to track the distribution of original creature colors across the colony. The color values should be represented as strings in the format `<red>_<green>_<blue>` (e.g., "255_128_64").

### Functional Requirements

1. **StatMetric enum extension**
   - Add `OriginalColor` variant to `StatMetric` enum in `crates/shared/src/be_api.rs`
   - The variant should follow the same pattern as existing metrics (Health, Size, CanKill, etc.)

1a. **StringStatBucket structure**
   - Add new `StringStatBucket` struct to `crates/shared/src/be_api.rs`:
     ```rust
     #[derive(Serialize, Deserialize, Debug, Clone)]
     pub struct StringStatBucket {
         pub value: String,
         pub occs: u64,
     }
     ```
   - Update `ShardStatResult` to include both numeric and string metrics:
     ```rust
     pub struct ShardStatResult {
         pub shard: Shard,
         pub metrics: Vec<(StatMetric, Vec<StatBucket>)>,
         pub string_metrics: Vec<(StatMetric, Vec<StringStatBucket>)>,
     }
     ```

2. **Backend stats computation**
   - Update `compute_stats` in `crates/backend/src/shard_utils.rs` to handle `StatMetric::OriginalColor`
   - Extract `original_color` from each cell (only for cells with `health > 0`)
   - Convert `Color` struct (red, green, blue u8 values) to string format: `format!("{}_{}_{}", color.red, color.green, color.blue)`
   - Create a new structure `StringStatBucket` with `value: String` and `occs: u64` for string-based metrics
   - Update `ShardStatResult` to support both `StatBucket` (for numeric metrics) and `StringStatBucket` (for string metrics like OriginalColor)
   - Return color strings directly in `StringStatBucket` values

3. **Coordinator stats aggregation**
   - Update `all_stat_metrics()` in `crates/coordinator/src/colony_stats.rs` to include `StatMetric::OriginalColor`
   - Update `enumerate_all_stat_metric_variants()` to include `OriginalColor` in the exhaustive match
   - Update `metric_id()` function to assign a unique ID to `OriginalColor` (e.g., 6)
   - Update the histogram building logic in `collect_statistics()` to:
     - Handle `OriginalColor` metric specially using `StringStatBucket` values directly
     - Aggregate color string counts from all shards
     - Add `original_color` field to `Histograms` struct
     - Include `original_color` histogram in the JSON output

4. **JSON output structure**
   - Add `original_color` field to `Histograms` struct in `crates/coordinator/src/colony_stats.rs`
   - The histogram distribution should use color strings as keys (e.g., `"255_128_64": 42`)
   - Follow same filtering rules: minimum 20 occurrences, top 20 values by count
   - No average calculation for OriginalColor - skip the `average` field entirely

5. **Test updates**
   - Update `test_all_stat_metrics_completeness` in `crates/coordinator/tests/test_colony_stats.rs` to ensure `OriginalColor` is included

### Technical Implementation Notes

1. **Histogram building**
   - Create `build_string_histogram()` function similar to `build_histogram()` but for string values
   - For `OriginalColor`, use `build_string_histogram()` which:
     - Filters by minimum 20 occurrences
     - Takes top 20 values by count
     - Skips the `average` field entirely (do not include it in the output)
     - Uses color strings directly as keys in the distribution map

