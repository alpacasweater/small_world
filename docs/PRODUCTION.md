# Production Guide

This document keeps deployment-focused details out of the top-level README.

## Validation approach

`small_world` does not rely only on analytic identities. It also runs differential checks against independent tools:

- PROJ `cct` for geodetic/topocentric/vertical transforms.
- GDAL `gdallocationinfo` for terrain interpolation comparisons.

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
cargo test
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
