small_world is a lightweight geodesy toolkit for robotics-oriented coordinate and height reference conversions.

The crate currently includes:
- WGS84 transform utilities (ECEF, NED, ENU, LLA/HAE internals).
- EGM geoid offset queries with explicit `EGM96` and `EGM2008` model options.
- SRTM `.hgt` terrain queries (nearest, bilinear, bicubic) with cross-tile seam-aware interpolation.
- Thread-safe bounded tile caching for production query workloads.
- Configurable void-sample handling policies for DEM gaps.
- A unified altitude conversion layer for `AGL <-> MSL <-> HAE`.

## Quick Start

1. Download geoid datasets (NGA source):

```bash
./scripts/download_geoid_data.sh --model all
```

Or model-specific:

```bash
./scripts/download_egm96.sh
./scripts/download_egm2008.sh
```

By default files are staged as:
- `data/WW15MGH.DAC` (EGM96)
- `data/EGM2008_2_5.DAC` (EGM2008)

For EGM96, the NGA source archive provides `WW15MGH.GRD` (ASCII). The download script automatically converts it into binary `WW15MGH.DAC` for fast runtime lookup.

2. Stage SRTM `.hgt` tiles in a directory (for example `data/srtm`):

- Tile naming is standard SRTM naming (`N39W077.hgt`, `S12E130.hgt`, etc).
- Heights are treated as terrain elevation above MSL.

3. Query geoid offset:

```bash
cargo run --example geoid_offset -- egm96 data/WW15MGH.DAC -0.466744 0.0023 bicubic
```

```bash
cargo run --example geoid_offset -- egm2008 data/EGM2008_2_5.DAC -0.466744 0.0023 bilinear
```

4. Convert between altitude references:

```bash
cargo run --example altitude_conversion -- egm96 data/WW15MGH.DAC data/srtm 39.0 -77.0 120 agl hae bilinear
```

5. Evaluate DEM accuracy against checkpoints (optional production gate):

```bash
cargo run --example ground_msl_accuracy -- data/srtm data/validation/ground_msl_checkpoints.csv bilinear 15.0 40.0
```

## Vertical Reference Frames

Every altitude must be interpreted in one explicit frame:

| Frame | Meaning | Zero level | Positive direction |
| --- | --- | --- | --- |
| `AGL` | Height above local terrain surface from DEM | Local ground at query lat/lon | Up from terrain |
| `MSL` | Orthometric height above mean sea level | Geoid surface | Up from geoid |
| `HAE` | Ellipsoidal height above WGS84 ellipsoid | WGS84 ellipsoid | Up from ellipsoid |

In this crate:
- `MSL` is orthometric height tied to the selected geoid model (`EGM96` or `EGM2008`).
- `HAE` is ellipsoidal height in the WGS84 ellipsoid frame.
- Terrain data (`SRTM .hgt`) is treated as `ground_msl`.

Typed API usage to prevent frame mixups:

```rust
use small_world::altitude::{AltitudeConverter, AltitudeSample, GeoPoint, VerticalFrame};

let point = GeoPoint::new(39.0, -77.0)?;
let input = AltitudeSample::agl_m(120.0)?; // explicitly AGL
let output = converter.convert_sample(point, input, VerticalFrame::Hae)?;
```

Terrain dataset hardening options:

```rust
use small_world::terrain::{SrtmDataset, VoidPolicy};

let terrain = SrtmDataset::new("data/srtm")
    .with_max_cached_tiles(256)
    .with_void_policy(VoidPolicy::NearestValid { max_radius_cells: 3 });
```

## Altitude Relationships

For any latitude/longitude:
- `HAE = MSL + geoid_offset(lat, lon)`
- `MSL = HAE - geoid_offset(lat, lon)`
- `MSL = terrain_msl(lat, lon) + AGL`
- `AGL = MSL - terrain_msl(lat, lon)`

Where:
- `geoid_offset(lat, lon)` comes from the selected geoid model (`EGM96` or `EGM2008`).
- `terrain_msl(lat, lon)` comes from the terrain DEM at that same geodetic point.

## Dataset Sources

NGA download links:
- EGM96 15-minute interpolation grid: https://earth-info.nga.mil/php/download.php?file=egm-96interpolation
- EGM2008 2.5-minute interpolation grid: https://earth-info.nga.mil/php/download.php?file=egm-08interpolation

Recommended terrain DEM options:
- SRTM GL1/GL3 (easy to obtain, simple `.hgt`, strong default for robotics/autonomy prototyping): https://www.earthdata.nasa.gov/data/catalog/lpcloud-srtmgl1n-003
- Copernicus DEM GLO-30/GLO-90 (higher quality and better high-latitude coverage; distributed as Cloud-Optimized GeoTIFF): https://registry.opendata.aws/copernicus-dem/
- USGS 3DEP (best choice for high-accuracy U.S.-only operations): https://www.usgs.gov/3d-elevation-program/about-3dep-products-services

The crate currently ships a native SRTM `.hgt` terrain reader and is designed so additional terrain backends can plug into the altitude conversion API.

## Validation and Fuzzing

- Property-based tests (`proptest`) validate randomized altitude conversion invariants.
- `cargo-fuzz` targets are provided under `fuzz/` for malformed terrain and geoid file inputs.
- CI includes:
  - stable test/lint/doc and runtime dependency gates
  - nightly fuzz smoke jobs
  - optional checkpoint-based terrain accuracy gate when `data/validation/ground_msl_checkpoints.csv` is provided

Run:

```bash
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Fuzz setup and commands:

```bash
cat fuzz/README.md
```

## Toolchain Policy

- Library and examples target stable Rust for end users.
- Nightly Rust is used only for fuzz validation (`cargo +nightly fuzz ...`) because sanitizer-based fuzzing requires nightly flags.
- The crate does not require users to switch their default toolchain to nightly.

## Dependency Footprint

- End-user runtime dependency set remains minimal: `byteorder` only.
- `proptest` is dev-only (`[dev-dependencies]`) and is not pulled by downstream crates using `small_world`.
- Fuzzing dependencies are isolated in `fuzz/Cargo.toml` and are not part of normal library builds.

## Notes

- Geoid longitudes are normalized internally to `[0, 360)`.
- SRTM longitudes are normalized internally to `[-180, 180)`.
- Latitudes must be finite and within `[-90, 90]`.
- Public geoid APIs return typed `Result` errors instead of panicking on invalid input.
