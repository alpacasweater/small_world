# Remediation Plan (codex branch)

## Agent Roles and Skills

Since this environment does not provide domain-specific installable skills beyond skill tooling, remediation is organized by specialist agent roles and the skills they require.

| Agent Role | Required Skills | Scope |
| --- | --- | --- |
| Build/CI Agent | CMake dependency graphing, GitHub Actions design, reproducible build validation | Fix stale binary risk and add CI verification |
| Performance Agent | Rust microbenchmark design, percentile metrics, throughput gating, Linux runner variance management | Add benchmark suite + performance gate |
| Geodesy Validation Agent | PROJ/GDAL differential testing, DEM sampling behavior, geoid model sanity checks | Add mandatory real-terrain oracle coverage |
| API Correctness Agent | Numeric validation APIs, backward-compatible constructor evolution, property tests | Add checked `wgs84` constructors + tests |
| FFI/ABI Agent | C ABI contract auditing, header/Rust symbol parity checks, compatibility policy | Add ABI drift detection |
| Data Integrity Agent | Cryptographic hash workflows, deterministic artifact logging, shell portability | Add checksum verification modes to download scripts |
| Docs/UX Agent | Frame-semantics clarity, example correctness, operator runbook quality | Fix example/docs inconsistencies and publish metrics |

## Metric-Driven Tasks

| ID | Task | Owner | Completion Metric |
| --- | --- | --- | --- |
| T1 | Eliminate stale Rust artifact risk in CMake integration | Build/CI Agent | CMake rebuild verification test passes and demonstrates stamp/artifact refresh after touching Rust source |
| T2 | Add performance benchmark and gate | Performance Agent | Bench/perf smoke outputs JSON metrics (`ops/s`, latency), CI enforces minimum thresholds |
| T3 | Make real-terrain oracle validation mandatory in CI | Geodesy Validation Agent | CI oracle stage includes real `.hgt` test and fails if missing/mismatched |
| T4 | Add checked WGS84 constructors | API Correctness Agent | `try_new` APIs implemented; invalid-input unit tests pass |
| T5 | Add ABI drift verification | FFI/ABI Agent | CI step compares exported `sw_*` symbols vs header declarations and fails on drift |
| T6 | Add dataset integrity verification options | Data Integrity Agent | Download scripts support strict checksum validation + hash logging; tests/docs updated |
| T7 | Final cleanup + metric report | Docs/UX Agent | Deficiency report updated with closure evidence and measured outputs |

## Done Criteria

All tasks are complete when:
1. `cargo fmt --all -- --check` passes.
2. `cargo test` passes (including oracle tests).
3. `cargo clippy --all-targets --all-features -- -D warnings` passes.
4. `cargo doc --no-deps` passes.
5. New CI steps for rebuild verification, perf gate, oracle real-terrain gate, and ABI drift check are present and green.
