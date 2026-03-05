#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_JSON="${OUT_JSON:-${ROOT_DIR}/target/perf_smoke_metrics.json}"

MIN_ALTITUDE_DATASET_OPS_PER_SEC="${MIN_ALTITUDE_DATASET_OPS_PER_SEC:-500000}"
MIN_TERRAIN_BILINEAR_OPS_PER_SEC="${MIN_TERRAIN_BILINEAR_OPS_PER_SEC:-500000}"
MIN_WGS84_ROUND_TRIP_OPS_PER_SEC="${MIN_WGS84_ROUND_TRIP_OPS_PER_SEC:-500000}"
MIN_FFI_SINGLE_THREAD_OPS_PER_SEC="${MIN_FFI_SINGLE_THREAD_OPS_PER_SEC:-250000}"
MIN_FFI_SHARED_8T_OPS_PER_SEC="${MIN_FFI_SHARED_8T_OPS_PER_SEC:-200000}"
MIN_FFI_PER_THREAD_8T_OPS_PER_SEC="${MIN_FFI_PER_THREAD_8T_OPS_PER_SEC:-500000}"
MIN_FFI_PER_THREAD_SCALE_VS_IDEAL="${MIN_FFI_PER_THREAD_SCALE_VS_IDEAL:-0.20}"
MAX_ALTITUDE_P95_NS="${MAX_ALTITUDE_P95_NS:-5000}"
MAX_FFI_P95_NS="${MAX_FFI_P95_NS:-8000}"
MAX_RSS_KB="${MAX_RSS_KB:-500000}"
REQUIRE_MAX_RSS="${REQUIRE_MAX_RSS:-1}"

mkdir -p "$(dirname "${OUT_JSON}")"

cargo run --release --example perf_smoke -- --json-out "${OUT_JSON}"

python3 - <<'PY' \
  "${OUT_JSON}" \
  "${MIN_ALTITUDE_DATASET_OPS_PER_SEC}" \
  "${MIN_TERRAIN_BILINEAR_OPS_PER_SEC}" \
  "${MIN_WGS84_ROUND_TRIP_OPS_PER_SEC}" \
  "${MIN_FFI_SINGLE_THREAD_OPS_PER_SEC}" \
  "${MIN_FFI_SHARED_8T_OPS_PER_SEC}" \
  "${MIN_FFI_PER_THREAD_8T_OPS_PER_SEC}" \
  "${MIN_FFI_PER_THREAD_SCALE_VS_IDEAL}" \
  "${MAX_ALTITUDE_P95_NS}" \
  "${MAX_FFI_P95_NS}" \
  "${MAX_RSS_KB}" \
  "${REQUIRE_MAX_RSS}"
import json
import sys
from pathlib import Path

json_path = Path(sys.argv[1])
min_altitude = float(sys.argv[2])
min_terrain = float(sys.argv[3])
min_wgs84 = float(sys.argv[4])
min_ffi_single = float(sys.argv[5])
min_ffi_shared_8t = float(sys.argv[6])
min_ffi_per_thread_8t = float(sys.argv[7])
min_ffi_scale = float(sys.argv[8])
max_altitude_p95_ns = float(sys.argv[9])
max_ffi_p95_ns = float(sys.argv[10])
max_rss_kb = float(sys.argv[11])
require_max_rss = bool(int(float(sys.argv[12])))

data = json.loads(json_path.read_text())
metrics = data["metrics"]
derived = data["derived"]
process = data.get("process", {})

altitude = metrics["altitude_dataset"]
terrain = metrics["terrain_bilinear"]
wgs84 = metrics["wgs84_round_trip"]
ffi_single = metrics["ffi_single_thread"]
ffi_shared = metrics["ffi_shared_handle_8t"]
ffi_per_thread = metrics["ffi_per_thread_handles_8t"]
ffi_scale = float(derived["ffi_per_thread_scale_vs_ideal"])

print("Performance gate metrics:")
print(
    f"  altitude_dataset          : {altitude['ops_per_sec']:.0f} ops/s, p95 {altitude['p95_ns_per_op']:.1f} ns/op"
)
print(
    f"  terrain_bilinear          : {terrain['ops_per_sec']:.0f} ops/s, p95 {terrain['p95_ns_per_op']:.1f} ns/op"
)
print(
    f"  wgs84_round_trip          : {wgs84['ops_per_sec']:.0f} ops/s, p95 {wgs84['p95_ns_per_op']:.1f} ns/op"
)
print(
    f"  ffi_single_thread         : {ffi_single['ops_per_sec']:.0f} ops/s, p95 {ffi_single['p95_ns_per_op']:.1f} ns/op"
)
print(
    f"  ffi_shared_handle_8t      : {ffi_shared['ops_per_sec']:.0f} ops/s, p95 {ffi_shared['p95_ns_per_op']:.1f} ns/op"
)
print(
    f"  ffi_per_thread_handles_8t : {ffi_per_thread['ops_per_sec']:.0f} ops/s, p95 {ffi_per_thread['p95_ns_per_op']:.1f} ns/op"
)
print(f"  ffi_per_thread_scale_ideal: {ffi_scale:.4f}")

failures = []
if float(altitude["ops_per_sec"]) < min_altitude:
    failures.append(
        f"altitude_dataset ops/s {altitude['ops_per_sec']:.0f} < {min_altitude:.0f}"
    )
if float(terrain["ops_per_sec"]) < min_terrain:
    failures.append(
        f"terrain_bilinear ops/s {terrain['ops_per_sec']:.0f} < {min_terrain:.0f}"
    )
if float(wgs84["ops_per_sec"]) < min_wgs84:
    failures.append(
        f"wgs84_round_trip ops/s {wgs84['ops_per_sec']:.0f} < {min_wgs84:.0f}"
    )
if float(ffi_single["ops_per_sec"]) < min_ffi_single:
    failures.append(
        f"ffi_single_thread ops/s {ffi_single['ops_per_sec']:.0f} < {min_ffi_single:.0f}"
    )
if float(ffi_shared["ops_per_sec"]) < min_ffi_shared_8t:
    failures.append(
        f"ffi_shared_handle_8t ops/s {ffi_shared['ops_per_sec']:.0f} < {min_ffi_shared_8t:.0f}"
    )
if float(ffi_per_thread["ops_per_sec"]) < min_ffi_per_thread_8t:
    failures.append(
        f"ffi_per_thread_handles_8t ops/s {ffi_per_thread['ops_per_sec']:.0f} < {min_ffi_per_thread_8t:.0f}"
    )
if ffi_scale < min_ffi_scale:
    failures.append(f"ffi_per_thread_scale_vs_ideal {ffi_scale:.4f} < {min_ffi_scale:.4f}")

if float(altitude["p95_ns_per_op"]) > max_altitude_p95_ns:
    failures.append(
        f"altitude_dataset p95 ns/op {altitude['p95_ns_per_op']:.1f} > {max_altitude_p95_ns:.1f}"
    )
if float(ffi_single["p95_ns_per_op"]) > max_ffi_p95_ns:
    failures.append(
        f"ffi_single_thread p95 ns/op {ffi_single['p95_ns_per_op']:.1f} > {max_ffi_p95_ns:.1f}"
    )

rss_kb = process.get("max_rss_kb", None)
if rss_kb is None:
    print("  max_rss_kb                : unavailable")
    if require_max_rss:
        failures.append("max_rss_kb unavailable while REQUIRE_MAX_RSS=1")
else:
    rss_kb = float(rss_kb)
    print(f"  max_rss_kb                : {rss_kb:.0f}")
    if rss_kb > max_rss_kb:
        failures.append(f"max_rss_kb {rss_kb:.0f} > {max_rss_kb:.0f}")

if failures:
    print("\nPerformance gate failed:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    raise SystemExit(1)

print("Performance gate passed.")
PY
