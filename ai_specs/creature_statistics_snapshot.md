## Spec Header

- **Spec name**: Periodic Creature Statistics Snapshot (Coordinator)
- **Spec status**: **draft**

---

## Main specification

### Goal

Whenever the coordinator performs its periodic **creature image capture** (currently once per minute), it must also produce a **statistics JSON file** for offline analysis and debugging, containing:

- Current global tick.  
- Histograms for: `creature_size`, `can_kill`, `can_move`.  
- A bounded history of recent colony events.  
- The current simulation rules (e.g. `"Health Cost Per Size Unit"` and other active knobs).

No new backend APIs may be added; all data must come from existing helpers / structures already used by the GUI.

### Functional Requirements

1. **Triggering and timing**
   - The statistics snapshot is created **once per image capture cycle** by the coordinator.
   - Stats use the **current tick at the time the stats run executes**; they are **not required** to share the exact same timestamp as the PNG, and small drift (a few seconds) is acceptable.

2. **Deployment modes**
   - The feature runs in **both AWS and localhost** modes, mirroring creature image capture behavior.

3. **Data contents**
   - **Tick**
     - JSON field `tick: number` representing the global simulation tick when stats were computed.
   - **Trait histograms**
     - Traits: `creature_size`, `can_kill`, `can_move`.
     - JSON shape:
       - `histograms.creature_size`: `{ "<size_value>": <count>, ... }`
       - `histograms.can_kill`: `{ "0": <count_false>, "1": <count_true>, ... }`
       - `histograms.can_move`: `{ "0": <count_false>, "1": <count_true>, ... }`
     - For boolean traits, keys `"0"` and `"1"` represent values encoded as `0` / `1` (false / true) as used by the GUI.
     - **Filter rule**: omit histogram entries whose `count` is **less than 20**; only values with at least 20 occurrences are included in the JSON.
     - Histograms must be computed using the **same semantics and data sources** as the GUI trait layers (no new meanings or logic).
   - **Event history**
     - JSON field `events: [ ... ]`.
     - Each entry is a **full event object** following the same schema and ordering used by the GUI’s colony events view.
     - Include up to the **last 20 events** available at the time of capture (fewer if less history exists), ordered consistently with the GUI (e.g. newest last).
   - **Rules**
     - JSON field `rules: { rule_name: value }`.
     - Keys are human-readable rule labels as shown in the GUI (e.g. `"Health Cost Per Size Unit"`).  
     - Values are the current configuration values (numeric / boolean / enum-like strings) from the live rules object.
   - **Metadata**
     - JSON field `meta`, e.g.:
       - `partial: bool` – `true` if some data could not be collected.  
       - `missing_shards: [string]` – identifiers of shards whose data was unavailable.  
       - `created_at_utc: string` – timestamp of when the stats file was produced.

4. **Storage and naming**
   - Stats files are written under `output/s3/distributed_colony/stats_shots/`.
   - Format is **UTF-8 JSON**.
   - Filenames are timestamp-based and compatible with image filenames, for example:
     - Images: `output/s3/distributed_colony/creatures_images/YYYY_MM_DD_HH_MM_SS.png`  
     - Stats: `output/s3/distributed_colony/stats_shots/YYYY_MM_DD_HH_MM_SS.json`  
   - Exact reuse of the image timestamp is **optional**; the implementation may either reuse the image timestamp when convenient or use an independent stats timestamp, with no strict alignment requirement.

5. **Error handling**
   - Any failure to compute stats or write the file:
     - Must be logged with current tick and error cause.  
     - Must **not** prevent PNG image capture or crash the coordinator.
   - If some shards or sources are unavailable:
     - Produce best-effort stats and reflect incompleteness via `meta.partial = true` and `meta.missing_shards`.

### Technical and Integration Notes

1. **Coordinator integration**
   - Extend the existing periodic capture task (the one that triggers creature image capture) to also call a **statistics capture helper** each cycle.
   - The helper:
     - Reads the current tick and colony topology from coordinator state.  
     - For each shard, reuses the same mechanisms used for GUI trait visualizations to obtain data for `creature_size`, `can_kill`, and `can_move`.  
     - Aggregates per-shard results into global histograms.

2. **Data sources (no new backend APIs)**
   - **Traits**: reuse the shard layer / trait mechanisms that currently power GUI layers for `creature_size`, `can_kill`, and `can_move`.  
   - **Events**: reuse the event log / store that backs the GUI’s colony events endpoint/view, then filter by the selected window.  
   - **Rules**: serialize from the same rules configuration object(s) that drive simulation behavior and are already visible in the GUI; do not duplicate definitions.

3. **Implementation details**
   - Ensure `output/s3/distributed_colony/stats_shots/` exists (create directories recursively if needed).  
   - Define small `serde::Serialize` structs for the top-level stats object, histograms, rules, and metadata, and serialize using `serde_json`.  
   - Use existing time / datetime utilities (e.g. `chrono` from `shared`) to generate `created_at_utc` and timestamp-based filenames.  
   - Once-per-minute execution can use blocking file I/O, provided it does not interfere with core coordinator responsibilities.

4. **Validation**
   - **Local**: confirm that for each PNG written to `output/s3/distributed_colony/creatures_images/`, a JSON file appears under `output/s3/distributed_colony/stats_shots/` with plausible tick, histograms, events, and rules.  
   - **AWS**: verify that S3 upload captures both PNG and JSON stats files, and that sampled PNG/JSON pairs show consistent tick, rules, and event windows with what the GUI displays during those periods.


