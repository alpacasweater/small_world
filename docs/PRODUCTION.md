# Production Guide

This document keeps deployment-focused details out of the top-level README.

## Validation approach

`small_world` does not rely only on analytic identities. It also runs differential checks against independent tools:

- PROJ `cct` for geodetic/topocentric/vertical transforms.
- GDAL `gdallocationinfo` for terrain interpolation comparisons.
- Mandatory real-terrain oracle checks using a checksum-pinned multi-region corpus:
  - `N39W077`, `N35E139`, `N37E127`, `S33E151`, `S22W043`, `N51E000`, `N27E086`
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
| Real-terrain `ground_msl`, geoid, and `MSL -> HAE` | GDAL + PROJ | 7 pinned SRTM tiles (`data/srtm`) + crate `WW15MGH.DAC` | 28 points | `<= 0.05 m` | ground `0.000000 m`, geoid `0.003713 m`, msl->hae `0.003713 m` |
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
- Perf harness reports peak RSS on Unix (`getrusage`) and Windows (`GetProcessMemoryInfo`).

### Performance gate baseline (2026-03-05 local run)

| Metric | Workload | CI threshold | Observed |
| --- | --- | --- | --- |
| `altitude_dataset` | `AGL -> HAE` with EGM96 bilinear + HGT bilinear | `>= 500,000 ops/s`, `p95 <= 5,000 ns` | `6,091,112 ops/s`, `p95 178.7 ns` |
| `terrain_bilinear` | HGT bilinear interpolation | `>= 500,000 ops/s` | `7,176,740 ops/s` |
| `wgs84_round_trip` | `NED -> LLA -> NED` | `>= 500,000 ops/s` | `7,245,495 ops/s` |
| `ffi_single_thread` | `sw_converter_convert_height_m` 1-thread | `>= 250,000 ops/s`, `p95 <= 8,000 ns` | `5,214,136 ops/s`, `p95 229.2 ns` |
| `ffi_shared_handle_8t` | 8 threads sharing one handle | `>= 200,000 ops/s` | `1,963,810 ops/s` |
| `ffi_per_thread_handles_8t` | 8 threads, one handle per thread | `>= 500,000 ops/s` | `11,054,257 ops/s` |
| `ffi_per_thread_scale_vs_ideal` | `ops_8t / (ops_1t * 8)` | `>= 0.20` | `0.2650` |
| `max_rss_kb` | Process peak RSS | `<= 500,000 kB` | `115,200 kB` |

## Confidence statement

- Absolute certainty is not possible for a global geospatial stack because upstream datasets, local terrain resolution, and interpolation behavior are approximations.
- This project targets high confidence instead:
  - explicit frame semantics (no implicit altitude-frame inference),
  - independent differential oracle checks (PROJ + GDAL),
  - deterministic pinned test datasets,
  - regression/performance gates in CI.

FFI usage recommendation:
- Shared-handle mode is safe but serialized due to per-handle mutex.
- For throughput-sensitive C/C++ callers, allocate one converter handle per worker thread.

Runtime dependency footprint is intentionally minimal:
- `byteorder` only.

Guardrail:

```bash
./scripts/check_runtime_dependencies.sh
```

## Python bindings

The `python/` directory contains a PyO3 extension module (`small_world._small_world`) built with Maturin.

### Building

```bash
pip install maturin
cd python
maturin develop --release        # editable install (dev workflow)
maturin build --release          # produce a wheel
```

### Running the Python test suite

```bash
cd python
# Point at your data files if available:
export SMALL_WORLD_EGM96=../data/WW15MGH.DAC
export SMALL_WORLD_SRTM=../data/srtm
pytest
```

Tests that require data files are automatically skipped when the files are absent.

### Python API surface

| Python class / enum | Rust source |
|---|---|
| `Lla`, `Ecef`, `Ned`, `Enu` | `src/wgs84.rs` |
| `EGM96`, `EGM2008` | `src/egm96.rs` |
| `SrtmDataset`, `VoidPolicy` | `src/terrain.rs` |
| `AltitudeConverter`, `TerrainReference` | `src/altitude.rs` |
| `VerticalFrame`, `Interpolation`, `GeoidModel` | enums |

All errors surface as `ValueError` with the same messages as the Rust `Display` impls.

Type stubs are provided at `python/small_world/_small_world.pyi` (PEP 561 compliant).

### Python dependencies

- `maturin >= 1.5` (build only)
- `pytest` (test only)
- No runtime Python dependencies beyond the compiled extension.

### Limitations

- `AltitudeConverter.set_void_policy()` rebuilds the terrain dataset (cache is cleared).
- The builder-pattern (`with_geoid_interpolation`) is replaced by mutating setters (`set_geoid_interpolation`).
- `EGM96` / `EGM2008` lazy-load tile data on first query; call `.load_data()` to pre-load for consistent latency.
- `Ned.from_ecef()` is not exposed in Python. Use `Enu.from_ecef(point, origin).to_ned(origin)` instead.

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

### C ABI function reference

| Group | Functions |
|---|---|
| Lifecycle | `sw_converter_options_default`, `sw_converter_create`, `sw_converter_destroy`, `sw_last_error_message` |
| Altitude → scalar | `sw_converter_convert_height_m`, `sw_converter_reference` |
| Altitude → LLA | `sw_converter_lla_wgs84_from_height_m` |
| Altitude → ECEF | `sw_converter_ecef_wgs84_from_height_m` |
| ECEF → altitude | `sw_converter_height_from_ecef_wgs84_m` |
| Diagnostics | `sw_converter_terrain_cache_stats` |
| LLA ↔ NED | `sw_wgs84_ned_to_lla`, `sw_wgs84_lla_to_ned` |
| LLA ↔ ECEF | `sw_wgs84_lla_to_ecef`, `sw_wgs84_ecef_to_lla` |
| NED ↔ ECEF | `sw_wgs84_ned_to_ecef`, `sw_wgs84_ecef_to_ned` |
| ENU ↔ LLA/NED/ECEF | `sw_wgs84_enu_to_lla`, `sw_wgs84_enu_to_ned_between_origins`, `sw_wgs84_enu_to_ecef`, `sw_wgs84_ecef_to_enu` |

The authoritative source is `include/small_world.h`.

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
- For `wgs84` types, prefer `try_new` when accepting external input. `new` constructors are intentionally unchecked.

## Dataset integrity options

- `scripts/download_geoid_data.sh` supports:
  - `--sha256-egm96`, `--sha256-egm2008`
  - `--strict-checksum`
  - `--sha256-log <path>`
- `scripts/download_hgt_tiles.sh` supports:
  - `--sha256-manifest <path>`
  - `--strict-checksum`
  - `--sha256-log <path>`
