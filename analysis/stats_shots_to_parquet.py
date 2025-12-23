#!/usr/bin/env python3
"""
Stats shots → Parquet converter for Distributed Colony.

Responsibilities:
- Discover colony IDs with stats shots in S3 (or process a single colony when requested).
- Download and parse stats shot JSON files (plain JSON or gzip-compressed).
- Normalize them into a wide, analytics-friendly tabular schema (one row per snapshot).
- Write a per-colony Parquet file under the local `output/analytics` directory.
- Optionally upload each Parquet file to S3 under `<colony_id>/stats_parquet/<colony_id>.parquet`.

Configuration is intentionally simple and mostly hard-coded, matching the spec.
"""

import argparse
import gzip
import io
import json
import os
import sys
from datetime import datetime, timezone
from typing import Any, Dict, Iterable, List, Optional, Tuple

import boto3
import pandas as pd


# --------------------------
# Hard-coded configuration
# --------------------------

BUCKET_NAME = "distributed-colony"
# In S3, stats shots live under:
#   <colony_id>/stats_shots/...
# e.g. s3://distributed-colony/tbk/stats_shots/2025_12_22__17_07_48.json
STATS_SHOTS_PREFIX = ""  # we derive <colony_id>/stats_shots/ per colony

# Local output directory for Parquet files (created if missing)
LOCAL_ANALYTICS_DIR = os.path.join("output", "analytics")

# S3 layout for Parquet outputs:
#   s3://distributed-colony/<colony_id>/stats_parquet/<colony_id>.parquet
PARQUET_S3_SUBPATH = "stats_parquet"


# --------------------------
# Utility helpers
# --------------------------

def log(msg: str) -> None:
    """Simple timestamped log to stdout."""
    ts = datetime.now(timezone.utc).isoformat()
    print(f"[{ts}] {msg}")


def read_s3_json(client, bucket: str, key: str) -> Dict[str, Any]:
    """
    Read a JSON object from S3, supporting both plain JSON and gzip-compressed JSON.

    Fails (raises) on any JSON parsing error, as per spec.
    """
    resp = client.get_object(Bucket=bucket, Key=key)
    body = resp["Body"].read()

    # Try to handle gzip transparently: attempt gzip decode first; if it fails,
    # treat the content as plain UTF-8 JSON.
    text: str
    try:
        with gzip.GzipFile(fileobj=io.BytesIO(body)) as gz:
            text = gz.read().decode("utf-8")
    except OSError:
        # Not gzip (or invalid gzip) – assume plain JSON text.
        text = body.decode("utf-8")

    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        # Surface malformed JSON immediately; caller will abort the run.
        raise ValueError(f"Malformed JSON in {bucket}/{key}: {exc}") from exc


def list_colony_ids(client, bucket: str, prefix: str) -> List[str]:
    """
    Discover colony IDs from keys shaped like:
      <colony_id>/stats_shots/...
    """
    log(f"Listing colony IDs under s3://{bucket}/{prefix or ''} (scanning for '<colony_id>/stats_shots/' prefixes)")
    paginator = client.get_paginator("list_objects_v2")
    colony_ids: set[str] = set()

    for page in paginator.paginate(Bucket=bucket, Prefix=prefix):
        for obj in page.get("Contents", []):
            key = obj["Key"]
            # Expect keys like: "<colony_id>/stats_shots/..."
            parts = key.split("/", 2)
            if len(parts) >= 2 and parts[1] == "stats_shots":
                colony_ids.add(parts[0])

    return sorted(colony_ids)


def list_stats_objects_for_colony(
    client, bucket: str, colony_id: str
) -> List[str]:
    """
    List all stats_shots keys for a given colony ID.
    """
    # Keys live under "<colony_id>/stats_shots/"
    prefix = f"{colony_id}/stats_shots/"
    log(f"[{colony_id}] Scanning S3 prefix s3://{bucket}/{prefix}")
    paginator = client.get_paginator("list_objects_v2")
    keys: List[str] = []

    for page in paginator.paginate(Bucket=bucket, Prefix=prefix):
        for obj in page.get("Contents", []):
            keys.append(obj["Key"])

    return sorted(keys)


# --------------------------
# Parquet schema helpers
# --------------------------

def _extract_rule_value(rules: Dict[str, Any], key: str) -> Optional[int]:
    val = rules.get(key)
    if val is None:
        return None
    try:
        return int(val)
    except (TypeError, ValueError):
        return None


def _summarize_creature_size(hist: Dict[str, Any]) -> Dict[str, Any]:
    """
    Compute total count, mean/avg, and a few percentiles over the creature_size histogram.
    """
    items: List[Tuple[int, int]] = []
    for k, v in hist.items():
        try:
            size = int(k)
            count = int(v)
        except (TypeError, ValueError):
            continue
        if count <= 0:
            continue
        items.append((size, count))

    if not items:
        return {
            "creature_count": 0,
            "creature_size_mean": None,
            "creature_size_avg": None,
            "creature_size_p50": None,
            "creature_size_p90": None,
            "creature_size_p99": None,
        }

    items.sort(key=lambda x: x[0])
    total = sum(c for _, c in items)
    total_weighted = sum(size * count for size, count in items)

    mean = total_weighted / total if total > 0 else None

    def percentile(p: float) -> Optional[float]:
        if total <= 0:
            return None
        threshold = total * p
        running = 0
        for size, count in items:
            running += count
            if running >= threshold:
                return float(size)
        return float(items[-1][0])

    p50 = percentile(0.5)
    p90 = percentile(0.9)
    p99 = percentile(0.99)

    return {
        "creature_count": total,
        "creature_size_mean": float(mean) if mean is not None else None,
        "creature_size_avg": float(mean) if mean is not None else None,
        "creature_size_p50": p50,
        "creature_size_p90": p90,
        "creature_size_p99": p99,
    }


def _summarize_boolean_hist(
    hist: Dict[str, Any],
    true_key: str = "1",
    false_key: str = "0",
    prefix: str = "can_kill",
) -> Dict[str, Any]:
    """
    Summarize a boolean histogram like:
      { "0": count_false, "1": count_true }
    """
    try:
        true_count = int(hist.get(true_key, 0))
    except (TypeError, ValueError):
        true_count = 0
    try:
        false_count = int(hist.get(false_key, 0))
    except (TypeError, ValueError):
        false_count = 0

    total = true_count + false_count
    if total > 0:
        frac_true = true_count / total
    else:
        frac_true = None

    return {
        f"{prefix}_true_count": true_count,
        f"{prefix}_false_count": false_count,
        f"{prefix}_true_fraction": float(frac_true) if frac_true is not None else None,
    }


def snapshot_to_row(snapshot: Dict[str, Any]) -> Dict[str, Any]:
    """
    Convert a single stats JSON snapshot into a flat row dict following the Parquet schema.
    """
    row: Dict[str, Any] = {}

    # Identity & core metadata
    row["colony_id"] = snapshot.get("colony_instance_id")
    row["tick"] = snapshot.get("tick")

    meta = snapshot.get("meta") or {}
    row["created_at_utc"] = meta.get("created_at_utc")

    # Rules
    rules = snapshot.get("rules") or {}
    row["rule_eat_capacity_per_size_unit"] = _extract_rule_value(
        rules, "Eat Capacity Per Size Unit"
    )
    row["rule_health_cost_if_can_kill"] = _extract_rule_value(
        rules, "Health Cost If Can Kill"
    )
    row["rule_health_cost_if_can_move"] = _extract_rule_value(
        rules, "Health Cost If Can Move"
    )
    row["rule_health_cost_per_size_unit"] = _extract_rule_value(
        rules, "Health Cost Per Size Unit"
    )
    row["rule_mutation_chance"] = _extract_rule_value(rules, "Mutation Chance")
    row["rule_random_death_chance"] = _extract_rule_value(rules, "Random Death Chance")

    # Histograms
    hists = snapshot.get("histograms") or {}
    creature_size_hist = hists.get("creature_size") or {}
    can_kill_hist = hists.get("can_kill") or {}
    can_move_hist = hists.get("can_move") or {}

    row.update(_summarize_creature_size(creature_size_hist))
    row.update(_summarize_boolean_hist(can_kill_hist, prefix="can_kill"))
    row.update(_summarize_boolean_hist(can_move_hist, prefix="can_move"))

    return row


# --------------------------
# Main processing
# --------------------------

def process_colony(
    client,
    colony_id: str,
    upload: bool,
) -> None:
    """
    Process all stats snapshots for a single colony:
    - Download & parse JSON
    - Normalize to rows
    - Write Parquet locally
    - Optionally upload Parquet to S3
    """
    keys = list_stats_objects_for_colony(client, BUCKET_NAME, colony_id)
    if not keys:
        log(f"[{colony_id}] No stats_shots objects found; skipping.")
        return

    log(f"[{colony_id}] Found {len(keys)} stats_shots objects.")
    rows: List[Dict[str, Any]] = []

    for key in keys:
        log(f"[{colony_id}] Reading {key}")
        snapshot = read_s3_json(client, BUCKET_NAME, key)
        row = snapshot_to_row(snapshot)
        if row.get("colony_id") != colony_id:
            # Be strict: mismatch between key path and payload colony_id is suspicious.
            raise ValueError(
                f"Colony ID mismatch for key {key}: "
                f"payload colony_instance_id={row.get('colony_id')}, expected {colony_id}"
            )
        rows.append(row)

    if not rows:
        raise RuntimeError(f"[{colony_id}] No rows produced from stats_shots JSON.")

    os.makedirs(LOCAL_ANALYTICS_DIR, exist_ok=True)
    local_path = os.path.join(LOCAL_ANALYTICS_DIR, f"{colony_id}.parquet")

    log(f"[{colony_id}] Writing Parquet to {local_path}")
    df = pd.DataFrame(rows)
    # Let pandas/pyarrow infer types; we rely on the schema definition in the spec.
    df.to_parquet(local_path, engine="pyarrow", compression="snappy", index=False)

    if upload:
        s3_key = f"{colony_id}/{PARQUET_S3_SUBPATH}/{colony_id}.parquet"
        log(f"[{colony_id}] Uploading Parquet to s3://{BUCKET_NAME}/{s3_key}")
        client.upload_file(local_path, BUCKET_NAME, s3_key)
    else:
        log(f"[{colony_id}] Upload disabled; Parquet only written locally.")


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Convert stats_shots JSON snapshots from S3 into Parquet for analytics. "
            "By default processes all colonies; use --colony-id to limit to one."
        )
    )
    parser.add_argument(
        "--colony-id",
        type=str,
        default=None,
        help="If provided, process only this colony ID instead of discovering all.",
    )
    parser.add_argument(
        "--upload",
        action="store_true",
        help=(
            "If set, upload the generated Parquet files to S3 under "
            "<colony_id>/stats_parquet/<colony_id>.parquet."
        ),
    )

    args = parser.parse_args(argv)

    s3_client = boto3.client("s3")

    try:
        if args.colony_id:
            colony_ids = [args.colony_id]
            log(f"Processing single colony_id={args.colony_id}")
        else:
            log("Discovering colony IDs from S3...")
            colony_ids = list_colony_ids(s3_client, BUCKET_NAME, STATS_SHOTS_PREFIX)
            log(f"Discovered {len(colony_ids)} colony IDs: {', '.join(colony_ids)}")

        if not colony_ids:
            log("No colonies found under stats_shots/; nothing to do.")
            return 0

        for colony_id in colony_ids:
            process_colony(s3_client, colony_id, upload=args.upload)

        log("All colonies processed successfully.")
        return 0

    except Exception as exc:
        # Fail fast on any data / JSON issues, per spec.
        log(f"ERROR: {exc}")
        return 1


if __name__ == "__main__":
    sys.exit(main())


