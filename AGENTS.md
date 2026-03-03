# AGENTS.md

## Purpose
Project handoff memory for `small_world`.
Use this file to restore context quickly, preserve API decisions, and keep frame semantics unambiguous.

## Snapshot (2026-03-03)
- Branch: `codex`
- Last known synced commit: `a44e1c5` on `origin/codex`
- Product direction: production-ready, converter-first altitude API

## What Was Decided

### 1) Altitude API choice is locked to Candidate A
Primary API surface:
- `AltitudeConverter::convert_height_m(point, height_m, from, to)`
- `AltitudeConverter::convert_sample(sample, to)`
- `AltitudeConverter::lla_wgs84_from_height_m(point, height_m, from)`
- `AltitudeConverter::lla_wgs84_from_sample(sample)`

Rationale:
- Most human-readable callsites
- Low ambiguity around source and destination vertical frame
- Least likely to hide bugs in implicit conversions

### 2) Candidate B/C APIs are intentionally removed
Do not reintroduce without explicit design review:
- `Lla::from_agl/from_msl/from_hae`
- fluent height helpers like `point.agl_m(...).convert_to(...)`
- `HeightAtPoint`
- `examples/minimal_api_ergonomics.rs`

### 3) Use explicit local/global structs
In `src/wgs84.rs`, preserve explicit type names and field names:
- `Lla { lat_deg, lon_deg, alt_m, alt_type }`
- `Ned { n, e, d, origin }`
- `Enu { e, n, u, origin }`

Expected primary methods:
- `Ned::new(...)`, `Ned::to_lla()`, `Ned::from_lla(...)`
- `Enu::new(...)`, `Enu::to_lla()`, `Enu::to_ned(...)`

Compatibility helpers (keep while needed by existing callsites):
- `enu_to_ned_between_origins(...)`
- `ned_to_lla_wgs84(...)`
- `lla_to_ned_wgs84(...)`

## Frame Semantics (Non-Negotiable)
- `AGL`: meters above local terrain.
- `MSL`: orthometric height above mean sea level (geoid-referenced).
- `HAE`: ellipsoidal height above WGS84 ellipsoid.
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

Git policy:
- Keep large datasets out of git history.
- `data/EGM2008_2_5.DAC` stays ignored in `.gitignore`.

## Code Map
- `src/altitude.rs`: frame conversion logic, converter entry points, altitude sample handling
- `src/wgs84.rs`: LLA/NED/ENU types and transforms
- `src/terrain.rs`: HGT tile loading, interpolation, void policy, caching
- `src/egm96.rs`: geoid grid readers and interpolation
- `src/height.rs`: interpolation options and height wrappers
- `src/lib.rs`: public exports
- `examples/minimal_frame_conversion.rs`: canonical concise user example
- `Readme.md`: user docs and conversion matrices

## Canonical Example Contract
`examples/minimal_frame_conversion.rs` should demonstrate, in minimal readable code:
1. ENU point with EGM2008-MSL origin -> NED point with EGM96-MSL origin
2. NED point with AGL origin -> LLA point in WGS84/HAE

If this example gets longer or less readable, refactor API/helpers before adding complexity.

## Quality Gates (Must Pass Before Push)
Run in this order:

```bash
cargo fmt --all -- --check
./scripts/check_runtime_dependencies.sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
```

Notes:
- Clippy warning policy is strict (`-D warnings`), including derivable impl linting.
- `cargo test` includes deterministic and property-based tests.
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
5. No stale references to removed Candidate B/C ergonomics.

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

## Resume Steps for the Next Agent
1. Check repo state: `git status -sb`.
2. Confirm docs and examples are Candidate A-only.
3. Run full quality gate commands.
4. Read `src/altitude.rs` and `src/wgs84.rs` before touching conversions.
5. If API changes are required, update:
   - tests
   - `examples/minimal_frame_conversion.rs`
   - `Readme.md`
   in the same commit.
