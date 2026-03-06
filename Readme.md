# small_world

`small_world` is a lightweight geodesy toolkit for robotics/autonomy that keeps altitude and frame semantics explicit.

## Why teams use it

- Explicit altitude frames (`AGL`, `MSL`, `HAE`) with no implicit guessing.
- Explicit local frames (`NED`, `ENU`, `LLA`) with readable structs and accessors.
- Lightweight runtime footprint (`byteorder` only).
- Differentially validated against trusted geospatial tools (PROJ + GDAL).
- C ABI and modern CMake support for C++ deployment.

## Add to your project

```toml
[dependencies]
small_world = { path = "../small_world" }
# or:
# small_world = { git = "https://github.com/Swarm-Command/small_world.git", branch = "main" }
```

## Frame contract

| Frame | Meaning | Zero level | Positive direction |
| --- | --- | --- | --- |
| `AGL` | Height above local terrain | Local DEM ground | Up |
| `MSL` | Orthometric height | Geoid surface | Up |
| `HAE` | Ellipsoidal height | WGS84 ellipsoid | Up |

Local Cartesian conventions:
- `NED`: `d` is positive down.
- `ENU`: `u` is positive up.

Global absolute frames:
- `LLA`: geodetic latitude/longitude degrees plus WGS84 `HAE` meters.
- `ECEF`: Earth-Centered, Earth-Fixed Cartesian meters on WGS84 axes.

## Quick start

1. Download geoid datasets (NGA):

```bash
./scripts/download_geoid_data.sh --model all
```

2. Download SRTM `.hgt` tiles for your area (5 GiB cap by default):

```bash
./scripts/download_hgt_tiles.sh \
  --lat-min 38.5 --lat-max 39.5 \
  --lon-min -77.6 --lon-max -76.2 \
  --out-dir data/srtm
```

3. Run the minimal example:

```bash
cargo run --example minimal_frame_conversion
```

4. Convert altitude frames in code:

```rust
use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::terrain::SrtmDataset;

let geoid = EGM96::new(std::path::Path::new("data/WW15MGH.DAC"))?;
let terrain = SrtmDataset::new("data/srtm");
let converter = AltitudeConverter::new(&geoid, &terrain);

let p = GeoPoint::new(39.0, -77.0)?;
let alt_hae_m = converter.convert_height_m(p, 120.0, VerticalFrame::Agl, VerticalFrame::Hae)?;
let p_ecef = converter.ecef_wgs84_from_height_m(p, 120.0, VerticalFrame::Agl)?;
```

## Core APIs

Altitude conversion API (converter-first):
- `convert_height_m(point, meters, from, to)`
- `convert_sample(point, sample, target_frame)`
- `lla_wgs84_from_height_m(point, meters, from)`
- `lla_wgs84_from_sample(point, sample)`
- `ecef_wgs84_from_height_m(point, meters, from)`
- `ecef_wgs84_from_sample(point, sample)`
- `height_from_ecef_wgs84_m(point_ecef_wgs84, target_frame)`
- `sample_from_ecef_wgs84(point_ecef_wgs84, target_frame)`

Local/global transforms:
- `Lla`, `Ecef`, `Ned`, `Enu` types with friendly accessors.
- `Lla::to_ecef`, `Lla::from_ecef`
- `Ned::to_lla`, `Ned::from_lla`, `Ned::to_ecef`, `Ned::from_ecef`
- `Enu::to_lla`, `Enu::to_ned`, `Enu::to_ecef`, `Enu::from_ecef`

## Validation and trust

This repo validates transformations against independent external oracles:
- PROJ `cct` (frame/vertical transforms)
- GDAL `gdallocationinfo` (terrain interpolation)
- Real-terrain oracle checks (not synthetic-only, multi-region corpus)
- Checksum-pinned real-terrain oracle tiles (`data/oracle_srtm_sha256.txt`)

Current real-terrain corpus spans 7 regions:
- `N39W077` (United States), `N35E139` (Japan), `N37E127` (South Korea)
- `S33E151` (Australia), `S22W043` (Brazil), `N51E000` (United Kingdom), `N27E086` (Nepal)

Current observed max error in oracle differential tests is millimeter-scale (for example `0.004438 m` in `AGL/MSL/HAE` matrix checks).

Accuracy note:
- No geospatial implementation can provide absolute certainty for every point on Earth.
- This crate provides high confidence through independent differential oracles and explicit frame semantics, with measured bounded error on validated corpora.

Performance is also gated in CI with dataset-backed workloads:
- Real `EGM96` + real `.hgt` code paths (no constant/mock providers)
- Throughput + p95 latency metrics for altitude, terrain, and WGS84 transforms
- FFI contention/scaling metrics (`1-thread`, `8-thread shared-handle`, `8-thread per-thread handles`)

C++ concurrency guidance:
- Shared converter handles are thread-safe but serialize on an internal mutex.
- For high throughput, use one converter handle per thread.

## More docs

- Production and validation details: [`docs/PRODUCTION.md`](docs/PRODUCTION.md)
- C++ and CMake integration walkthrough: [`examples/cpp/README.md`](examples/cpp/README.md)
- Canonical compact example: [`examples/minimal_frame_conversion.rs`](examples/minimal_frame_conversion.rs)
- Performance gate runner: [`scripts/run_perf_smoke.sh`](scripts/run_perf_smoke.sh)
- C ABI/header sync checker: [`scripts/verify_c_header_sync.sh`](scripts/verify_c_header_sync.sh)
