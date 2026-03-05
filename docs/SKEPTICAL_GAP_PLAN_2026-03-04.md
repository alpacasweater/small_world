# Skeptical Readiness Review and Execution Plan (2026-03-04)

## Review Goal

Assess whether `small_world` currently satisfies all of the following in production terms:
- lightweight
- cross-platform
- user-friendly/ergonomic
- resource efficient
- extremely performant

## Execution status

Execution of this plan is complete. Closure evidence and measured outputs are captured in:
- `docs/DEFICIENCY_CLOSURE_2026-03-04.md`
- `target/perf_smoke_metrics.json` (from `./scripts/run_perf_smoke.sh`)

## What is already strong

- Runtime dependency footprint is minimal (`byteorder` only).
- Frame semantics are explicit and strongly tested.
- Differential oracle validation exists against respected independent tools (PROJ + GDAL).
- C API/header sync now has an automated parity check.

## Remaining Deficiencies (Skeptical View)

### D1 (P1): Cross-platform evidence is incomplete

Current CI runs only on `ubuntu-latest`; no Windows/macOS test lanes exist for Rust + CMake + C ABI integration.

Evidence:
- `.github/workflows/ci.yml` has one job with `runs-on: ubuntu-latest`.

Risk:
- Platform-specific regressions in FFI/CMake/linking can ship undetected.

---

### D2 (P1): Performance gate does not yet represent end-user workloads

`perf_smoke` benchmarks use constant/mock geoid/terrain providers for altitude conversion, which can overstate throughput and does not model real dataset IO/cache behavior.

Evidence:
- `ConstGeoid` / `ConstTerrain` benchmark path in `examples/perf_smoke.rs:22-49,98-114`.

Risk:
- Reported “extremely performant” figures may not predict performance for actual EGM/SRTM datasets.

---

### D3 (P1): Resource-usage metric is not reliably captured cross-platform

Perf gate prints `max_rss_kb: unavailable` on this host because `/usr/bin/time` option detection fails; memory budget is therefore unenforced.

Evidence:
- `scripts/run_perf_smoke.sh` currently depends on `/usr/bin/time` flags not universally available.
- Observed output includes `max_rss_kb : unavailable`.

Risk:
- Regressions in memory footprint can pass CI unnoticed.

---

### D4 (P2): FFI concurrency scalability remains unproven

The C ABI uses a per-handle mutex around converter operations. There is no benchmark gate for handle contention under multi-threaded C++ usage.

Evidence:
- `Mutex<ConverterCore>` and lock usage in `src/ffi.rs`.

Risk:
- Throughput collapses for shared-handle multi-threaded consumers.

---

### D5 (P2): Oracle CI depends on live network fetch for real-terrain tile

`run_oracle_validation.sh` fetches `N39W077.hgt` on demand if missing. This improves realism but introduces network flakiness and source mutability risk unless checksums are pinned in the oracle path.

Evidence:
- `scripts/run_oracle_validation.sh` auto-download block.

Risk:
- Non-deterministic CI failures and reduced reproducibility.

## Metric-Driven Remediation Plan

| Task | Owner Sub-Agent | Required Skills | Success Metrics |
| --- | --- | --- | --- |
| T1: Add multi-OS CI matrix (Linux/macOS/Windows) for core build+test and CMake smoke | Platform Agent | GitHub Actions matrix design, cross-platform toolchain setup, CMake packaging | Matrix green on all 3 OSes for `fmt`, `check`, `test`, CMake C++ smoke build |
| T2: Replace synthetic-only perf gate with realistic dataset-backed perf benchmarks | Performance Agent | Rust benchmarking, cache-aware workload modeling, statistical benchmarking | Add benchmark suite with real EGM96 + real SRTM tile; enforce min throughput and max p95 latency thresholds |
| T3: Make memory metrics portable and mandatory | Performance Agent | Cross-platform process metrics collection, CI artifacting | Perf gate outputs RSS on Linux/macOS/Windows and enforces `max_rss_kb` threshold |
| T4: Add FFI multi-thread throughput benchmark and contention budget | FFI Performance Agent | C++/C ABI load testing, threading, lock-contention analysis | Publish single-thread vs 8-thread throughput; enforce min scaling ratio (e.g. >=0.6x ideal with per-thread handles) |
| T5: Make real-terrain oracle deterministic (checksum-pinned) | Validation Agent | Data integrity checks, oracle test reproducibility | Oracle script verifies pinned SHA256 for test tile; CI oracle stage independent of mutable remote content |

## Sub-Agent Execution Packets

### Sub-Agent A: Platform Agent

Objective:
- Add cross-platform confidence.

Skills:
- GitHub Actions matrix design
- Rust toolchain/target setup
- CMake cross-platform linking

Deliverables:
1. `ci.yml` matrix for `ubuntu-latest`, `macos-latest`, `windows-latest`.
2. CMake C++ smoke compile on each platform.
3. Platform-specific notes in docs.

Acceptance metrics:
- 3/3 OS lanes green for build/test gates.

---

### Sub-Agent B: Performance Agent

Objective:
- Produce credible, dataset-backed performance evidence.

Skills:
- Rust benchmark design (Criterion or equivalent)
- Cache/warmup design
- Quantile analysis and regression gating

Deliverables:
1. Bench harness for:
   - `AGL<->MSL<->HAE` conversion (real EGM+SRTM)
   - terrain interpolation throughput
   - `LLA<->NED` throughput
2. JSON metrics with `ops/s`, `p50/p95/p99` latency.
3. CI perf gate with explicit thresholds.

Acceptance metrics:
- Thresholds enforced in CI; artifacted benchmark JSON per run.

---

### Sub-Agent C: FFI Performance Agent

Objective:
- Validate and improve C++ integration throughput under concurrency.

Skills:
- C++ multithread load generation
- FFI overhead profiling
- Lock contention analysis

Deliverables:
1. C++ benchmark executable for `sw_converter_convert_height_m` and `sw_converter_reference`.
2. Comparison of shared-handle vs per-thread-handle throughput.
3. Recommended usage policy or architecture patch.

Acceptance metrics:
- Publish scaling table for 1/2/4/8 threads.
- CI smoke threshold for minimum QPS at 8 threads.

---

### Sub-Agent D: Validation Agent

Objective:
- Ensure real-terrain oracle path is deterministic and reproducible.

Skills:
- Test data pinning
- Checksum workflows
- Oracle test curation

Deliverables:
1. Pinned checksum manifest for oracle terrain tiles.
2. Oracle script uses strict checksum mode for required tile(s).
3. Failure mode docs for checksum mismatch.

Acceptance metrics:
- Oracle stage fails on checksum mismatch.
- Oracle stage succeeds with pinned artifacts only.

---

### Sub-Agent E: Docs/UX Agent

Objective:
- Keep user onboarding ergonomic while adding rigor.

Skills:
- Technical documentation design
- API ergonomics communication
- Runbook authoring

Deliverables:
1. Update README quick-start for multi-OS notes.
2. Production guide section for performance SLO interpretation.
3. Short troubleshooting section for CI/perf/oracle failures.

Acceptance metrics:
- New user can run quick-start on each supported platform in <=15 minutes.

## Recommended Next Execution Order

1. T1 (multi-OS CI) 
2. T5 (deterministic oracle data) 
3. T2 + T3 (credible perf + portable memory metrics)
4. T4 (FFI concurrency scaling)
5. Documentation polish and release checklist update
