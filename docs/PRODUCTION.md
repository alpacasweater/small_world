# Production Guide

This document keeps deployment-focused details out of the top-level README.

## Validation approach

`small_world` does not rely only on analytic identities. It also runs differential checks against independent tools:

- PROJ `cct` for geodetic/topocentric/vertical transforms.
- GDAL `gdallocationinfo` for terrain interpolation comparisons.
- Mandatory real-terrain oracle checks using `data/srtm/N39W077.hgt` during oracle validation.
- Real-terrain tile integrity is checksum-pinned by `data/oracle_srtm_sha256.txt`.

References:
- PROJ `cct` docs: <https://proj.org/en/stable/apps/cct.html>
- PROJ project: <https://proj.org/>
- PROJ OSGeo page: <https://www.osgeo.org/projects/proj/>
- GDAL `gdallocationinfo` docs: <https://gdal.org/en/stable/programs/gdallocationinfo.html>

### Oracle accuracy summary (current)

| Transformation | Oracle | Inputs | Corpus | Threshold | Max observed error |
| --- | --- | --- | --- | --- | --- |
| `AGL/MSL/HAE` conversion matrix | PROJ `cct` + GDAL | Geoid: `us_nga_egm96_15.tif`, terrain: synthetic SRTM tile + crate `WW15MGH.DAC` | 864 conversions | `<= 0.05 m` | `0.004438 m` |
| `lla_wgs84_from_height_m` altitude output | PROJ `cct` + GDAL | Same as above | 288 cases | `<= 0.05 m` | `0.004438 m` |
| `LLA(HAE) -> NED` | PROJ `cct` topocentric | WGS84 | 288 cases | `<= 0.04 m/component` | `0.000000 m` |
| `NED -> LLA(HAE)` | PROJ `cct` topocentric | WGS84 | 288 cases | horiz `<= 0.03 m`, vert `<= 0.03 m` | horiz `0.000000 m`, vert `0.000000 m` |
| `ENU(origin A) -> NED(origin B)` | PROJ pipeline via absolute LLA | WGS84 | 192 cases | `<= 0.04 m/component` | `0.000000 m` |
| Pole/anti-meridian edge cases | PROJ `cct` topocentric | WGS84 | 12 cases | horiz `<= 0.03 m`, vert `<= 0.03 m` | horiz `0.000000 m`, vert `0.000000 m` |

## Runtime and performance notes

- Terrain cache is bounded, thread-safe, and configurable.
- Terrain interpolation is seam-aware across tile boundaries.
- Void handling is explicit via `VoidPolicy`.
- FFI converter handles reuse loaded geoid/terrain state.
- Performance smoke metrics are emitted as JSON (`target/perf_smoke_metrics.json`) and gated in CI.
- Performance gate runs dataset-backed paths (EGM96 + `.hgt`) and tracks p95 latency.
- FFI gate includes contention/scaling metrics for shared-handle vs per-thread-handle usage.

### Performance gate baseline (2026-03-04 local run)

| Metric | Workload | CI threshold | Observed |
| --- | --- | --- | --- |
| `altitude_dataset` | `AGL -> HAE` with EGM96 bilinear + HGT bilinear | `>= 500,000 ops/s`, `p95 <= 5,000 ns` | `5,955,627 ops/s`, `p95 197.4 ns` |
| `terrain_bilinear` | HGT bilinear interpolation | `>= 500,000 ops/s` | `6,709,314 ops/s` |
| `wgs84_round_trip` | `NED -> LLA -> NED` | `>= 500,000 ops/s` | `6,692,134 ops/s` |
| `ffi_single_thread` | `sw_converter_convert_height_m` 1-thread | `>= 250,000 ops/s`, `p95 <= 8,000 ns` | `4,617,946 ops/s`, `p95 278.7 ns` |
| `ffi_shared_handle_8t` | 8 threads sharing one handle | `>= 200,000 ops/s` | `1,217,080 ops/s` |
| `ffi_per_thread_handles_8t` | 8 threads, one handle per thread | `>= 500,000 ops/s` | `9,238,814 ops/s` |
| `ffi_per_thread_scale_vs_ideal` | `ops_8t / (ops_1t * 8)` | `>= 0.20` | `0.2501` |
| `max_rss_kb` | Process peak RSS | `<= 500,000 kB` | `62,320 kB` |

FFI usage recommendation:
- Shared-handle mode is safe but serialized due to per-handle mutex.
- For throughput-sensitive C/C++ callers, allocate one converter handle per worker thread.

Runtime dependency footprint is intentionally minimal:
- `byteorder` only.

Guardrail:

```bash
./scripts/check_runtime_dependencies.sh
```

## C/C++ integration

Artifacts and headers:
- Header: `include/small_world.h`
- Static lib: `target/release/libsmall_world.a`
- Shared lib: `target/release/libsmall_world.{so|dylib|dll}`

Build:

```bash
cargo build --release
```

Modern CMake helper:
- `cmake/SmallWorldRust.cmake`
- Link target: `small_world::small_world`

See full integration example: `examples/cpp/README.md`

### ABI compatibility policy

- Exported `sw_*` function signatures in `include/small_world.h` are treated as the public C ABI contract.
- Any breaking signature change must be accompanied by a major-version compatibility decision and release notes.
- CI enforces symbol/header parity via `./scripts/verify_c_header_sync.sh`.

## Dataset sources

Geoid (NGA):
- EGM96 interpolation grid: <https://earth-info.nga.mil/php/download.php?file=egm-96interpolation>
- EGM2008 interpolation grid: <https://earth-info.nga.mil/php/download.php?file=egm-08interpolation>

Terrain options:
- SRTM GL1/GL3: <https://www.earthdata.nasa.gov/data/catalog/lpcloud-srtmgl1n-003>
- Copernicus DEM GLO-30/GLO-90: <https://registry.opendata.aws/copernicus-dem/>
- USGS 3DEP: <https://www.usgs.gov/3d-elevation-program/about-3dep-products-services>

## Recommended release gates

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

Optional nightly validation (not required by end users):
- `cargo +nightly fuzz run ...`

## Operational safety notes

- Keep frame suffixes in variable names (`alt_msl_m`, `alt_hae_m`, `height_agl_m`).
- Never mix `MSL` and `HAE` arithmetic without explicit conversion.
- In this crate, SRTM `.hgt` elevations are terrain `MSL`.
- `NED.d` is down; `ENU.u` is up.

## Dataset integrity options

- `scripts/download_geoid_data.sh` supports:
  - `--sha256-egm96`, `--sha256-egm2008`
  - `--strict-checksum`
  - `--sha256-log <path>`
- `scripts/download_hgt_tiles.sh` supports:
  - `--sha256-manifest <path>`
  - `--strict-checksum`
  - `--sha256-log <path>`
