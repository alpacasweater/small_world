# small_world Deficiency Report (2026-03-04)

## Review Method

I ran this review using five specialist lenses:
- Performance engineering
- Geodesy/oracle validation
- FFI/ABI integration
- Build/CI and release engineering
- Supply-chain/data integrity

This report focuses on deployment-readiness deficiencies (bugs, risks, missing evidence), ordered by severity.

## Findings (Ordered by Severity)

### P1 — CMake integration can produce stale Rust binaries

**Why this matters**
- `small_world_add_rust_library(...)` uses a stamp-file custom command without source dependencies.
- After the first build, CMake can skip rebuilding Rust even when `src/*.rs` changes.
- This can ship stale transform code into C++ applications.

**Evidence**
- Custom command writes stamp and has no `DEPENDS` on Rust source graph: `cmake/SmallWorldRust.cmake:71-82`.

**Recommendation**
- Add proper dependencies (`Cargo.toml`, `Cargo.lock`, `src/**`, `include/small_world.h`, optionally `build.rs`) to the custom command.
- Or replace the stamp approach with a custom target that always runs `cargo build` and relies on Cargo’s incremental rebuild logic.
- Add CI check that edits a Rust file and verifies CMake target rebuilds the Rust artifact.

---

### P1 — No production-grade performance metrics or CI performance gates

**Why this matters**
- Current performance confidence is based on a single unit test with a loose wall-clock assertion (`<10s`), not on reproducible throughput/latency/memory metrics.
- There is no benchmark suite, no percentile latency tracking, and no regression budget in CI.

**Evidence**
- Only perf-like guard is one test with a broad threshold: `src/terrain.rs:925-958`.
- CI has no benchmark/perf stage: `.github/workflows/ci.yml:29-47`.
- No benchmark crate/harness configured: `Cargo.toml:1-17`.

**Current ad-hoc datapoint (non-authoritative)**
- `terrain::tests::high_volume_queries_reuse_cache` (100k bilinear queries) finished in ~0.39s in debug and ~0.02s in release on this machine.
- This is not a production metric because the test uses synthetic data and does not record memory, tail latency, or cross-platform results.

**Recommendation**
- Add reproducible benchmarks (Criterion or iai-callgrind) for:
  - `AGL<->MSL<->HAE` conversion throughput/latency
  - terrain query interpolation modes
  - FFI API throughput (single-thread + multi-thread)
- Track at minimum: p50/p95/p99 latency, QPS, CPU%, RSS.
- Add CI perf smoke gate with tolerated regression budgets (for example ±10%).

---

### P1 — Oracle altitude validation in CI is synthetic-terrain only

**Why this matters**
- Trusted external oracle tests are excellent, but terrain oracle coverage in CI uses a generated linear `.hgt` tile.
- That validates transform plumbing, not real-world terrain complexity (voids, steep gradients, seam artifacts, noisy DEM behavior).

**Evidence**
- Synthetic tile generation in oracle test: `tests/oracle_altitude_external.rs:67-77` and usage `:192-196`.
- Optional real-world accuracy gate is skipped unless user-provided data exists: `.github/workflows/ci.yml:41-47`.

**Recommendation**
- Add a small, committed oracle checkpoint set (lat/lon + expected GDAL/PROJ values) from real terrain tiles.
- Make one real-world oracle test mandatory in CI (lightweight sample count), keep larger suites optional/nightly.

---

### P2 — FFI query path is serialized per converter handle

**Why this matters**
- A single `SwConverterHandle` uses a `Mutex<ConverterCore>`; every conversion acquires that lock.
- Multi-threaded C++ callers sharing one handle are serialized, which can become a throughput bottleneck.

**Evidence**
- Handle-level mutex: `src/ffi.rs:185`.
- Lock held in conversion/reference calls: `src/ffi.rs:497-505`, `541-549`, `595-603`, `636-644`.

**Recommendation**
- Document this behavior explicitly in C API docs.
- Add benchmark results showing scaling with 1, 2, 4, 8 threads.
- Consider architecture change:
  - immutable/shared geoid + sharded terrain caches, or
  - per-thread handle guidance with pooling utilities.

---

### P2 — `wgs84` public constructors do not validate coordinates

**Why this matters**
- `Lla::new`, `Ned::new`, `Enu::new` accept arbitrary `f64` (including invalid lat/lon and NaN).
- Invalid values can silently propagate into transforms and produce undefined operational behavior.

**Evidence**
- `Lla::new` has no validation: `src/wgs84.rs:36-45`.
- In contrast, altitude-side `GeoPoint::new` validates bounds: `src/altitude.rs:35-45`.

**Recommendation**
- Introduce checked constructors (`try_new`) with explicit error types.
- Keep `new` only if needed for const contexts, but mark as unchecked and add prominent docs.
- Add negative tests for NaN/infinite/out-of-range coordinates in `wgs84` API.

---

### P2 — No automated ABI/header drift check for C interface

**Why this matters**
- C header is manually maintained.
- Changes in `src/ffi.rs` can drift from `include/small_world.h` without automated detection.

**Evidence**
- Public header exists: `include/small_world.h:1-126`.
- No cbindgen/bindgen generation or verification in repo/CI (`Cargo.toml` and `.github/workflows/ci.yml` contain no header-generation step).

**Recommendation**
- Add automated header generation/verification (for example `cbindgen --verify` in CI).
- Version the ABI and publish compatibility policy (major/minor guarantees).

---

### P3 — Data download scripts do not verify checksums/signatures

**Why this matters**
- Downloads are retrieved over HTTPS, but integrity is not cryptographically verified.
- Tampered/mis-served files could pass through and impact transformation outputs.

**Evidence**
- Geoid script uses `curl` without checksum/signature verification: `scripts/download_geoid_data.sh:146-147`.
- HGT script uses `curl` without checksum/signature verification: `scripts/download_hgt_tiles.sh:126-130`.

**Recommendation**
- Add optional strict mode requiring SHA256 checks against pinned manifests.
- Log hash of staged artifacts and keep in CI artifacts for traceability.

---

### P3 — Example comment/frame mismatch could confuse users

**Why this matters**
- C++ example comment says AGL but the call uses `SW_FRAME_MSL`.
- This undermines the explicit-frame safety principle in docs.

**Evidence**
- Comment says AGL: `examples/cpp/minimal_conversion.cpp:26`.
- Actual argument is `SW_FRAME_MSL`: `examples/cpp/minimal_conversion.cpp:28-30`.

**Recommendation**
- Fix comment or change call to match the stated frame.

## Non-Finding Notes

- Differential oracle strategy (PROJ + GDAL) is strong and well above typical project rigor.
- Frame-explicit altitude APIs are clear and reduce class-of-bug risk.
- Runtime dependency minimization is enforced by script and CI.

## Suggested Next Actions (Priority Order)

1. Fix CMake stale-binary risk (`SmallWorldRust.cmake` dependencies).
2. Add benchmark suite + CI perf gate (latency/QPS/RSS targets).
3. Make at least one real-world oracle checkpoint test mandatory in CI.
4. Add checked constructors (`try_new`) for `wgs84` types.
5. Add ABI/header drift verification in CI.
6. Add checksum verification modes for dataset download scripts.
7. Correct C++ example comment/frame mismatch.
