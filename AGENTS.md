# AGENTS.md

Engineering guide for contributors and coding agents working on `small_world`.
It records the API decisions that are locked, the frame semantics that are
non-negotiable, and the quality gates every change must pass.

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
- `MSL`: orthometric height above a **named geoid model** — `VerticalFrame::Msl(EgmModel)`.
  Converters reject `Msl` tags that differ from their geoid (`GeoidModelMismatch`) and `AGL`
  conversions over a terrain datum that differs from the geoid (`TerrainDatumMismatch`).
  Never add an untagged MSL path; the C ABI's untagged `SW_FRAME_MSL` resolves to the
  converter's own model, which keeps it unambiguous by construction. Cross-model movement is
  explicit via `GeoidShift` (HAE-invariant: `MSL_to = MSL_from + N_from − N_to`); it is
  Rust-only for now — a C ABI surface for it needs a two-handle design (lock ordering!) and
  should be its own reviewed addition.
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

Missing-data UX policy:
- A missing geoid file must fail with `EgmError::DatasetMissing`, whose message contains the
  exact fetch command (`EgmModel::download_command()`). Keep that command in sync with
  `scripts/download_geoid_data.sh`, and keep the EGM96 arm advertising `embedded-egm96`.

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
- `src/geoid.rs`: geoid grid readers and interpolation
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
- `README.md`: user docs and conversion matrices

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

## Licensing and Release

- Dual licensed `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE`); contributions are
  accepted under the same terms.
- `data/WW15MGH.DAC` is U.S. Government data (NGA/NASA EGM96), not covered by the code
  licenses — provenance lives in `NOTICE`, and any change touching the data or its embedding
  must keep `NOTICE` accurate.
- The `embedded-egm96` feature embeds that grid via `include_bytes!`; it must stay optional so
  default builds carry no data payload.
- Published crate contents are governed by `package.exclude` in `Cargo.toml`; repo-internal
  material (this file, `docs/`, `.github/`, `fuzz/`) stays out of the package.
