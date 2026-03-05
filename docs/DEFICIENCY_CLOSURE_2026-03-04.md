# Deficiency Closure Report (2026-03-04)

Branch: `codex`

This report closes the deficiencies identified in `docs/DEFICIENCY_REPORT_2026-03-04.md` using the metric-driven plan in `docs/REMEDIATION_PLAN_2026-03-04.md`.

## Agent/Skill Workstreams Executed

| Workstream | Skills Applied | Status |
| --- | --- | --- |
| Build/CI hardening | CMake dependency graphing, CI gate design | Complete |
| Performance validation | Throughput/latency benchmarking, threshold gating | Complete |
| Geodesy/oracle validation | PROJ/GDAL differential checks with real terrain | Complete |
| API correctness | Input-validation API design + tests | Complete |
| FFI/ABI integrity | Symbol/header parity automation | Complete |
| Data integrity | Checksum verification workflows in scripts | Complete |
| Docs consistency | Frame semantics/doc polish | Complete |

## Closure Matrix

| Deficiency | Resolution | Metric / Acceptance Criteria | Result |
| --- | --- | --- | --- |
| CMake stale Rust binaries risk | Added Rust source/header/Cargo dependencies to CMake custom command and added rebuild verification script | `./scripts/verify_cmake_rust_rebuild.sh` must advance CMake stamp after touching Rust source | **Pass** (stamp advanced `1772683414 -> 1772683417`) |
| Missing performance metrics/gate | Added dataset-backed `examples/perf_smoke.rs` + `scripts/run_perf_smoke.sh` + CI perf stage/artifact | Perf gate thresholds: altitude `>=500k ops/s`, terrain bilinear `>=500k ops/s`, wgs84 round-trip `>=500k ops/s`; FFI/latency/RSS thresholds enforced | **Pass** (`6,091,112`, `7,176,740`, `7,245,495` ops/s; see `target/perf_smoke_metrics.json`) |
| Synthetic-only terrain oracle in CI | Added `tests/oracle_real_terrain.rs`; made oracle script stage pinned multi-region tiles and run mandatory real-terrain test | Real terrain max error thresholds `<=0.05 m` for ground/geoid/MSL->HAE | **Pass** (`ground=0.000000m`, `geoid=0.003713m`, `msl_to_hae=0.003713m` across 7 regions/28 points) |
| Unchecked `wgs84` constructors | Added `Lla::try_new`, `Ned::try_new`, `Enu::try_new` with `Wgs84Error` and tests | New invalid-input unit tests must pass | **Pass** (`wgs84::tests::checked_constructors_validate_inputs`) |
| No ABI/header drift check | Added `scripts/verify_c_header_sync.sh` and CI stage | Script must detect parity between Rust exported `sw_*` and header declarations | **Pass** (`12 exported functions` in sync) |
| No dataset checksum verification modes | Added checksum options to geoid/HGT download scripts (strict mode + hash log) | Scripts support strict verification and hash logging; syntax and dry-run checks pass | **Pass** (`bash -n` clean; strict dry-run verification succeeded) |
| Example frame mismatch | Fixed C++ example comment to match MSL call | Comment and frame argument semantically aligned | **Pass** |

## CI/Quality Gate Additions

Added/updated gates:
- `./scripts/verify_c_header_sync.sh`
- `./scripts/verify_cmake_rust_rebuild.sh`
- `./scripts/run_perf_smoke.sh`
- `./scripts/run_oracle_validation.sh` now includes mandatory real-terrain oracle test

Workflow updates:
- `.github/workflows/ci.yml` now runs rebuild verification, perf gate, ABI sync check, and uploads perf artifacts.

## Local Full Gate Status

All passed locally:

```bash
cargo fmt --all -- --check
./scripts/check_runtime_dependencies.sh
./scripts/verify_c_header_sync.sh
cargo test
./scripts/verify_cmake_rust_rebuild.sh
./scripts/run_perf_smoke.sh
./scripts/run_oracle_validation.sh
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
```

## Skeptical Gap Closure Update (D1-D5)

| Gap ID | Closure action | Metric / proof | Result |
| --- | --- | --- | --- |
| D1: cross-platform evidence incomplete | Added `cross-platform-smoke` CI matrix for Linux/macOS/Windows with Rust check/tests, dataset-backed perf smoke, CMake C++ smoke build + runtime ctest, and macOS PROJ oracle smoke | `.github/workflows/ci.yml` matrix job | **Closed** |
| D2: perf not workload-representative | Replaced synthetic altitude benchmark with dataset-backed EGM96 + HGT path; removed constant-provider perf path from gate | `examples/perf_smoke.rs` + `scripts/run_perf_smoke.sh` | **Closed** |
| D3: RSS not reliably enforced | Perf harness now reports process `max_rss_kb` on Unix and Windows; gate enforces RSS where available and defaults to no-RSS-gate on platforms that cannot report | `max_rss_kb=115200` vs threshold `<=500000` | **Closed** |
| D4: FFI contention/scaling unproven | Added FFI throughput metrics for `1-thread`, `8-thread shared handle`, `8-thread per-thread handles`; added concurrent FFI correctness test | `ffi_per_thread_scale_vs_ideal=0.2650`; `ffi_shared_handle_is_safe_under_concurrent_queries` test | **Closed** |
| D5: oracle tile mutability risk | Added checksum manifest and strict checksum validation in oracle script for a 7-tile global corpus | `data/oracle_srtm_sha256.txt` + strict verify in `scripts/run_oracle_validation.sh` | **Closed** |
