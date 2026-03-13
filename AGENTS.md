# AGENTS.md

## Purpose
Project handoff memory for `small_world`.
Use this file to restore context quickly, preserve API decisions, and keep frame semantics unambiguous.

## Snapshot (2026-03-05)
- Branch: `main`
- Last known synced commit: `8a9790f` on `origin/main`
- Product direction: production-ready, converter-first altitude API

## What Was Decided

### 1) Altitude API choice is locked to converter-first API
Primary API surface:
- `AltitudeConverter::convert_height_m(point, height_m, from, to)`
- `AltitudeConverter::convert_sample(point, sample, to)`
- `AltitudeConverter::lla_wgs84_from_height_m(point, height_m, from)`
- `AltitudeConverter::lla_wgs84_from_sample(point, sample)`
- `AltitudeConverter::ecef_wgs84_from_height_m(point, height_m, from)`
- `AltitudeConverter::ecef_wgs84_from_sample(point, sample)`
- `AltitudeConverter::height_from_ecef_wgs84_m(point_ecef_wgs84, to)`
- `AltitudeConverter::sample_from_ecef_wgs84(point_ecef_wgs84, to)`

Rationale:
- Most human-readable callsites
- Low ambiguity around source and destination vertical frame
- Least likely to hide bugs in implicit conversions

### 2) Older implicit/construction-style APIs are intentionally removed
Do not reintroduce without explicit design review:
- `Lla::from_agl/from_msl/from_hae`
- fluent height helpers like `point.agl_m(...).convert_to(...)`
- `HeightAtPoint`
- `examples/minimal_api_ergonomics.rs`

### 3) Use explicit local/global structs
In `src/wgs84.rs`, preserve explicit type names and accessor names:
- `Lla` with `lat_deg()`, `lon_deg()`, `alt_m()`, `alt_type()`
- `Ecef` with `x()`, `y()`, `z()`
- `Ned` with `n()`, `e()`, `d()`, `origin()`
- `Enu` with `e()`, `n()`, `u()`, `origin()`

Expected primary methods:
- `Lla::to_ecef()`, `Lla::from_ecef(...)`
- `Lla::try_new(...)` for checked construction; `Lla::new(...)` is unchecked
- `Ned::new(...)`, `Ned::to_lla()`, `Ned::from_lla(...)`
- `Ned::try_new(...)` for checked construction
- `Ned::to_ecef()`, `Ned::from_ecef(...)`
- `Enu::new(...)`, `Enu::to_lla()`, `Enu::to_ned(...)`
- `Enu::try_new(...)` for checked construction
- `Enu::to_ecef()`, `Enu::from_ecef(...)`

Compatibility helpers (keep while needed by existing callsites):
- `enu_to_ned_between_origins(...)`
- `ned_to_lla_wgs84(...)`
- `lla_to_ned_wgs84(...)`

## Frame Semantics (Non-Negotiable)
- `AGL`: meters above local terrain.
- `MSL`: orthometric height above mean sea level (geoid-referenced).
- `HAE`: ellipsoidal height above WGS84 ellipsoid.
- `ECEF`: Earth-Centered, Earth-Fixed Cartesian meters on WGS84 axes.
- `NED`: `d` is positive down.
- `ENU`: `u` is positive up.

Rules:
- Never infer frame from position alone; pass frame enums explicitly.
- Never rename/alias frames in docs without precise definitions.
- Every user-facing example must identify the frame of each altitude value.

## Data Sources and Policy
- Geoid grids:
  - EGM96: `data/WW15MGH.DAC`
  - EGM2008: `data/EGM2008_2_5.DAC`
- Terrain:
  - SRTM HGT through `SrtmDataset`
  - Oracle real-terrain validation corpus pinned by `data/oracle_srtm_sha256.txt`:
    - `N39W077`, `N35E139`, `N37E127`, `S33E151`, `S22W043`, `N51E000`, `N27E086`

Git policy:
- Keep large datasets out of git history.
- `data/EGM2008_2_5.DAC` stays ignored in `.gitignore`.

## C/C++ ABI Surface
- C ABI entry points live in `src/ffi.rs`.
- Public C header lives in `include/small_world.h`.
- CMake helper for consumers lives in `cmake/SmallWorldRust.cmake`.
- Build outputs include `staticlib` and `cdylib` for downstream C++ link targets.
- C ABI keeps frame semantics explicit:
  - `SwVerticalFrame`: `AGL`, `MSL`, `HAE`
  - `SwLlaWgs84.hae_m`: always WGS84 ellipsoidal altitude
  - `SwEcef`: always WGS84 Cartesian meters (`x_m`, `y_m`, `z_m`)
  - `SwNed`: `n_m`, `e_m`, `d_m` (d positive down)
  - `SwEnu`: `e_m`, `n_m`, `u_m` (u positive up)
- Opaque handle `SwConverterHandle` owns geoid/terrain state for efficient repeated queries.
- Do not add implicit frame conversion APIs to C ABI without explicit design review.

Current public `sw_*` functions (authoritative: `include/small_world.h`):
- Lifecycle: `sw_converter_options_default`, `sw_converter_create`, `sw_converter_destroy`, `sw_last_error_message`
- Altitude→scalar: `sw_converter_convert_height_m`, `sw_converter_reference`
- Altitude→LLA: `sw_converter_lla_wgs84_from_height_m`
- Altitude↔ECEF: `sw_converter_ecef_wgs84_from_height_m`, `sw_converter_height_from_ecef_wgs84_m`
- Diagnostics: `sw_converter_terrain_cache_stats`
- LLA↔NED: `sw_wgs84_ned_to_lla`, `sw_wgs84_lla_to_ned`
- LLA↔ECEF: `sw_wgs84_lla_to_ecef`, `sw_wgs84_ecef_to_lla`
- NED↔ECEF: `sw_wgs84_ned_to_ecef`, `sw_wgs84_ecef_to_ned`
- ENU↔*: `sw_wgs84_enu_to_lla`, `sw_wgs84_enu_to_ned_between_origins`, `sw_wgs84_enu_to_ecef`, `sw_wgs84_ecef_to_enu`

## Code Map
- `src/altitude.rs`: frame conversion logic, converter entry points, altitude sample handling
- `src/wgs84.rs`: LLA/ECEF/NED/ENU types and transforms
- `src/terrain.rs`: HGT tile loading, interpolation, void policy, caching
- `src/egm96.rs`: geoid grid readers and interpolation
- `src/height.rs`: interpolation options and height wrappers
- `src/lib.rs`: public exports
- `src/ffi.rs`: C ABI bridge for C/C++ consumers
- `include/small_world.h`: C/C++ header for ABI consumption
- `cmake/SmallWorldRust.cmake`: modern CMake integration helper
- `examples/minimal_frame_conversion.rs`: canonical concise user example
- `examples/cpp/minimal_conversion.cpp`: minimal C++ integration example
- `examples/cpp/CMakeLists.txt`: working modern CMake C++ integration example
- `tests/oracle_altitude_external.rs`: trusted external oracle tests for `AGL/MSL/HAE` (PROJ + GDAL)
- `tests/oracle_proj.rs`: independent PROJ (`cct`) differential oracle tests
- `tests/oracle_altitude.rs`: supplemental analytic invariant tests for frame algebra
- `scripts/download_hgt_tiles.sh`: bbox-based SRTM `.hgt` downloader with size cap (default 5 GiB)
- `scripts/run_oracle_validation.sh`: local/CI oracle validation entrypoint
- `Readme.md`: user docs and conversion matrices

### Python bindings (`python/`)
- `python/Cargo.toml`: `small_world_py` crate — PyO3 extension (`_small_world.cdylib`)
- `python/pyproject.toml`: Maturin build config, pytest settings
- `python/src/lib.rs`: all PyO3 `#[pyclass]`/`#[pymethods]` bindings; `OwnedGeoid` enum for lifetime-free `AltitudeConverter` ownership
- `python/small_world/__init__.py`: re-exports from the compiled extension
- `python/small_world/_small_world.pyi`: PEP 561 type stubs for all public classes
- `python/tests/conftest.py`: data-availability fixtures and skip markers
- `python/tests/test_wgs84.py`: WGS84 frame conversion tests (no data files required)
- `python/tests/test_altitude.py`: altitude converter and geoid tests (skipped when data absent)
- `python/examples/minimal_frame_conversion.py`: LLA/ECEF/ENU/NED demo
- `python/examples/altitude_conversion.py`: AGL→MSL→HAE CLI demo

## CI Topology
- `stable-checks` (Ubuntu): full gates including oracle + perf + docs.
- `cross-platform-smoke` (Linux/macOS/Windows):
  - `cargo check`, `cargo test --lib`
  - dataset-backed `perf_smoke` example run
  - CMake C++ build + `ctest` runtime smoke
  - macOS lane also runs PROJ oracle smoke (`oracle_proj`)

## Canonical Example Contract
`examples/minimal_frame_conversion.rs` should demonstrate, in minimal readable code:
1. ENU point with EGM2008-MSL origin -> NED point with EGM96-MSL origin
2. NED point with AGL origin -> LLA point in WGS84/HAE
3. Explicit frame altitude -> WGS84/ECEF -> target vertical frame round trip

If this example gets longer or less readable, refactor API/helpers before adding complexity.

## Quality Gates (Must Pass Before Push)
Run in this order:

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

Notes:
- Clippy warning policy is strict (`-D warnings`), including derivable impl linting.
- `cargo test` includes deterministic and property-based tests.
- `./scripts/run_oracle_validation.sh` runs trusted external oracle tests (PROJ + GDAL) and is required for release confidence.
- Oracle gate prerequisites: `cct` (PROJ) and `gdallocationinfo` (GDAL) available on PATH.
- Keep regression tests for known conversion edge cases and frame mix-ups.

## Fuzzing and Validation Policy
- End-user build/toolchain: stable only.
- Validation can use nightly (CI or local), but nightly is not a runtime requirement.
- Fuzz targets live in `fuzz/` and should focus on:
  - malformed/edge HGT tiles
  - geoid grid indexing/interpolation boundaries
  - conversion round-trip invariants
- Scheduled fuzz checks are configured in `.github/workflows/nightly-fuzz.yml`.

## Dependency Policy
- Runtime dependency footprint must remain minimal.
- `./scripts/check_runtime_dependencies.sh` is the guardrail.
- Runtime expectation remains very small (currently `byteorder` at runtime).
- Additional crates are acceptable only in `dev-dependencies` or fuzz-only scope.

## Production Readiness Checklist
Before declaring production-ready:
1. All quality gates pass locally.
2. CI passes on PR branch (including clippy strict mode).
3. README and example code reflect current public API exactly.
4. Public API docs and method names are frame-explicit.
5. No stale references to removed implicit ergonomics.

## Common Failure Modes
- Ambiguous variable names like `alt` without frame suffix.
- Silent frame translation inside constructors.
- Mixing `MSL` and `HAE` in arithmetic without conversion.
- Treating `NED.d` as up.
- Adding convenience APIs that obscure source/target frame.

## Naming Conventions
- Prefer frame-suffixed names in code/examples:
  - `alt_msl_m`, `alt_hae_m`, `height_agl_m`
  - `origin_msl`, `origin_hae`
- Prefer methods that spell intent:
  - `convert_height_m(..., from, to)` over implicit `convert(...)`

## jcodemunch — Mandatory Code Intelligence Tool

**All agents working in this repo must use jcodemunch before reading files directly.**
jcodemunch is indexed at `local/small_world-ba36cdc8`. Using it saves significant tokens (~$0.25–$0.40/session vs. reading raw files).

### Workflow rules

1. **Re-index first** (incremental — fast):
   ```
   mcp__jcodemunch__index_folder(path="<repo_root>", incremental=true)
   ```
   Do this at the start of every session and after making file changes.

2. **Look up symbols before reading files.** Use these tools in order of preference:
   - `search_symbols(query, repo)` — find functions/types by name or concept
   - `get_file_outline(repo, file_path)` — get all symbols in a file with signatures
   - `get_symbol(repo, symbol_id)` — fetch a single symbol's full body
   - Only fall back to `Read` when you need complete file context (e.g., test file bodies, config files with no symbols)

3. **Never read an entire source file to find a function.** Use `search_symbols` instead.

4. **Keep the index current.** After adding, renaming, or deleting files run `index_folder` again with `incremental=true`.

### Quick reference

| Task | Tool |
|---|---|
| Find where `convert_height_m` is defined | `search_symbols(query="convert_height_m")` |
| List all symbols in `src/altitude.rs` | `get_file_outline(file_path="src/altitude.rs")` |
| See the full body of a specific function | `get_symbol(symbol_id=<id from outline>)` |
| Explore the full repo structure | `get_repo_outline(repo="local/small_world-ba36cdc8")` |
| Find all callers of a function | `find_references(symbol_id=...)` |

### Current index state (2026-03-12)
- Repo ID: `local/small_world-ba36cdc8`
- Symbols: 681 (Rust × 21 files, Python × 5 files, C/C++ × 3 files, Bash × 9, Python scripts × 1)
- Re-indexing takes < 1 second (incremental)

---

## Resume Steps for the Next Agent
1. Check repo state: `git status -sb`.
2. Confirm docs and examples are converter-first only.
3. Run full quality gate commands.
4. Read `src/altitude.rs` and `src/wgs84.rs` before touching conversions.
5. If API changes are required, update:
   - tests
   - `examples/minimal_frame_conversion.rs`
   - `python/src/lib.rs` (Python bindings)
   - `python/small_world/_small_world.pyi` (type stubs)
   - `Readme.md`
   in the same commit.

### Python bindings quality gate
After any change to Rust sources, rebuild and re-test Python bindings:

```bash
cd python
maturin develop
pytest
```

Verify that WGS84 round-trip tests pass without data files, and altitude tests skip gracefully when data is absent.
