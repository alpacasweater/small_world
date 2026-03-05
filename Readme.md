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
# small_world = { git = "https://github.com/Swarm-Command/small_world.git", branch = "codex" }
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
```

## Core APIs

Altitude conversion API (converter-first):
- `convert_height_m(point, meters, from, to)`
- `convert_sample(point, sample, target_frame)`
- `lla_wgs84_from_height_m(point, meters, from)`
- `lla_wgs84_from_sample(point, sample)`

Local/global transforms:
- `Lla`, `Ned`, `Enu` types with friendly accessors (`.n()`, `.e()`, `.d()`, `.u()`).
- `Ned::to_lla`, `Ned::from_lla`, `Enu::to_lla`, `Enu::to_ned`.

## Validation and trust

This repo validates transformations against independent external oracles:
- PROJ `cct` (frame/vertical transforms)
- GDAL `gdallocationinfo` (terrain interpolation)

Current observed max error in oracle differential tests is millimeter-scale (for example `0.004438 m` in `AGL/MSL/HAE` matrix checks).

## More docs

- Production and validation details: [`docs/PRODUCTION.md`](docs/PRODUCTION.md)
- C++ and CMake integration walkthrough: [`examples/cpp/README.md`](examples/cpp/README.md)
- Canonical compact example: [`examples/minimal_frame_conversion.rs`](examples/minimal_frame_conversion.rs)
