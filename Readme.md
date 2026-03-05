# small_world

`small_world` is a lightweight geodesy toolkit for robotics/autonomy applications that need explicit, trustworthy altitude/frame transforms.

It is designed around one rule: every altitude value must carry an explicit reference frame (`AGL`, `MSL`, `HAE`).

## Why adopt this library

- Explicit frame conversions: no hidden frame assumptions in altitude APIs.
- Lightweight runtime dependencies: `byteorder` only.
- Real-world friendly data sources: NGA geoid grids + SRTM `.hgt` terrain.
- Differential validation against respected external tools (PROJ + GDAL).
- C ABI included for C/C++ integration.

## Add to your project

`small_world` can be consumed as a path dependency (local checkout) or a git dependency.

```toml
[dependencies]
small_world = { path = "../small_world" }
# or:
# small_world = { git = "https://github.com/Swarm-Command/small_world.git", branch = "codex" }
```

## What Frames Mean

| Frame | Meaning | Zero level | Positive direction |
| --- | --- | --- | --- |
| `AGL` | Height above local terrain surface | Local DEM ground at query point | Up |
| `MSL` | Orthometric height above mean sea level | Geoid surface | Up |
| `HAE` | Ellipsoidal height above WGS84 ellipsoid | WGS84 ellipsoid | Up |

Local Cartesian conventions:
- `NED`: `d` is positive down.
- `ENU`: `u` is positive up.

## 5-Minute Quick Start (Rust)

1. Download geoid data (NGA):

```bash
./scripts/download_geoid_data.sh --model all
```

2. Download terrain tiles for your operating area (defaults to a 5 GiB safety cap):

```bash
./scripts/download_hgt_tiles.sh \
  --lat-min 38.5 --lat-max 39.5 \
  --lon-min -77.6 --lon-max -76.2 \
  --out-dir data/srtm
```

3. Run the minimal frame/altitude conversion example:

```bash
cargo run --example minimal_frame_conversion
```

4. Perform a direct altitude-frame conversion in your code:

```rust
use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::egm96::EGM96;
use small_world::terrain::SrtmDataset;

let geoid = EGM96::new(std::path::Path::new("data/WW15MGH.DAC"))?;
let terrain = SrtmDataset::new("data/srtm");
let converter = AltitudeConverter::new(&geoid, &terrain);

let p = GeoPoint::new(39.0, -77.0)?;
let hae_m = converter.convert_height_m(p, 120.0, VerticalFrame::Agl, VerticalFrame::Hae)?;
```

## Primary Altitude API (Candidate A)

This project intentionally standardizes on converter-first calls:

- `AltitudeConverter::convert_height_m(point, height_m, from, to)`
- `AltitudeConverter::convert_sample(sample, to)`
- `AltitudeConverter::lla_wgs84_from_height_m(point, height_m, from)`
- `AltitudeConverter::lla_wgs84_from_sample(sample)`

### Minimal conversion matrix

Given:

```rust
use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};

let p = GeoPoint::new(lat_deg, lon_deg)?;
let c = AltitudeConverter::new(&geoid, &terrain);
let h = input_m;
```

| from \\ to | `AGL` | `MSL` | `HAE` |
| --- | --- | --- | --- |
| `AGL` | `h` | `c.convert_height_m(p, h, VerticalFrame::Agl, VerticalFrame::Msl)?` | `c.convert_height_m(p, h, VerticalFrame::Agl, VerticalFrame::Hae)?` |
| `MSL` | `c.convert_height_m(p, h, VerticalFrame::Msl, VerticalFrame::Agl)?` | `h` | `c.convert_height_m(p, h, VerticalFrame::Msl, VerticalFrame::Hae)?` |
| `HAE` | `c.convert_height_m(p, h, VerticalFrame::Hae, VerticalFrame::Agl)?` | `c.convert_height_m(p, h, VerticalFrame::Hae, VerticalFrame::Msl)?` | `h` |

### Frame equations used

For any `(lat, lon)` query:
- `HAE = MSL + geoid_offset(lat, lon)`
- `MSL = HAE - geoid_offset(lat, lon)`
- `MSL = ground_msl(lat, lon) + AGL`
- `AGL = MSL - ground_msl(lat, lon)`

## LLA / NED / ENU API

Explicit structs avoid tuple-index mistakes:

```rust
use small_world::wgs84::{AltType, Enu, Lla, Ned};

let origin = Lla::new(39.0, -77.0, 150.0, AltType::Wgs84);
let enu = Enu::new(15.0, -4.0, 3.0, origin);
let ned = Ned::new(40.0, -8.0, 6.0, origin);

let lla_from_ned = ned.to_lla();
let ned_from_enu = enu.to_ned(origin);
let _accessors = (enu.e(), enu.n(), enu.u(), ned.n(), ned.e(), ned.d(), lla_from_ned.alt_m());
```

Compatibility helpers remain available:
- `enu_to_ned_between_origins(...)`
- `ned_to_lla_wgs84(...)`
- `lla_to_ned_wgs84(...)`

## Trust and Validation

`small_world` validation is not limited to algebraic identity checks.
It performs differential tests against independent external tools:

- PROJ `cct` for geodetic/topocentric and vertical grid-shift transforms.
- GDAL `gdallocationinfo` for terrain interpolation oracle checks.

References:
- PROJ `cct` docs: <https://proj.org/en/stable/apps/cct.html>
- PROJ project: <https://proj.org/>
- PROJ OSGeo project page: <https://www.osgeo.org/projects/proj/>
- GDAL `gdallocationinfo`: <https://gdal.org/en/stable/programs/gdallocationinfo.html>

### Oracle accuracy summary (current)

These are measured errors from local oracle tests (not just pass/fail):

| Transformation | Oracle | Inputs | Corpus | Threshold | Max observed error |
| --- | --- | --- | --- | --- | --- |
| `AGL/MSL/HAE` conversion matrix | PROJ `cct` + GDAL | Geoid: `us_nga_egm96_15.tif`, terrain: synthetic SRTM tile + crate `WW15MGH.DAC` | 864 conversions | `<= 0.05 m` | `0.004438 m` |
| `lla_wgs84_from_height_m` altitude output | PROJ `cct` + GDAL | Same as above | 288 cases | `<= 0.05 m` | `0.004438 m` |
| `LLA(HAE) -> NED` | PROJ `cct` topocentric | WGS84 | 288 cases | `<= 0.04 m/component` | `0.000000 m` |
| `NED -> LLA(HAE)` | PROJ `cct` topocentric | WGS84 | 288 cases | horiz `<= 0.03 m`, vert `<= 0.03 m` | horiz `0.000000 m`, vert `0.000000 m` |
| `ENU(origin A) -> NED(origin B)` | PROJ pipeline via absolute LLA | WGS84 | 192 cases | `<= 0.04 m/component` | `0.000000 m` |
| Pole/anti-meridian edge cases | PROJ `cct` topocentric | WGS84 | 12 cases | horiz `<= 0.03 m`, vert `<= 0.03 m` | horiz `0.000000 m`, vert `0.000000 m` |

Run the external-oracle validation suite:

```bash
./scripts/run_oracle_validation.sh
```

Run the real `.hgt` comparison example:

```bash
cargo run --example validate_real_hgt_against_oracles
```

## Performance Notes

- Terrain tile cache is bounded and thread-safe.
- Cross-tile bilinear/bicubic interpolation is seam-aware.
- Void sample behavior is explicit via `VoidPolicy`.
- Converter handles in FFI reuse loaded geoid/terrain datasets.

Cache policy example:

```rust
use small_world::terrain::{SrtmDataset, VoidPolicy};

let terrain = SrtmDataset::new("data/srtm")
    .with_max_cached_tiles(256)
    .with_void_policy(VoidPolicy::NearestValid { max_radius_cells: 3 });
```

## Lightweight Dependency Footprint

Runtime dependency set is intentionally minimal:
- `byteorder` only.

Validation/fuzz dependencies are isolated to test/dev/fuzz contexts and are not runtime requirements for downstream users.

Guardrail script:

```bash
./scripts/check_runtime_dependencies.sh
```

## C/C++ Integration (Deployment Friendly)

C ABI artifacts and header:
- Header: `include/small_world.h`
- Static: `target/release/libsmall_world.a`
- Shared: `target/release/libsmall_world.{so|dylib|dll}`

Build release artifacts:

```bash
cargo build --release
```

### Modern CMake integration

Use helper module `cmake/SmallWorldRust.cmake` and link `small_world::small_world`.

Subdirectory workflow:

```cmake
cmake_minimum_required(VERSION 3.21)
project(my_robot_app LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/third_party/small_world/cmake")
include(SmallWorldRust)

small_world_add_rust_library(
  TARGET small_world_ffi
  MANIFEST_DIR "${CMAKE_CURRENT_SOURCE_DIR}/third_party/small_world"
  PROFILE release
  LINKAGE STATIC
)

add_executable(my_app src/main.cpp)
target_link_libraries(my_app PRIVATE small_world::small_world)
```

See `examples/cpp/README.md` for a full end-to-end CMake example.

## Dataset Sources

Geoid (NGA):
- EGM96 interpolation grid: <https://earth-info.nga.mil/php/download.php?file=egm-96interpolation>
- EGM2008 interpolation grid: <https://earth-info.nga.mil/php/download.php?file=egm-08interpolation>

Terrain options:
- SRTM GL1/GL3: <https://www.earthdata.nasa.gov/data/catalog/lpcloud-srtmgl1n-003>
- Copernicus DEM GLO-30/GLO-90: <https://registry.opendata.aws/copernicus-dem/>
- USGS 3DEP: <https://www.usgs.gov/3d-elevation-program/about-3dep-products-services>

## Recommended Pre-Deployment Quality Gates

Run in this order:

```bash
cargo fmt --all -- --check
./scripts/check_runtime_dependencies.sh
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps
```

Optional nightly validation (not required by end users):
- `cargo +nightly fuzz run ...`

## Important Operational Notes

- Always carry frame labels in variable names (`alt_msl_m`, `height_agl_m`, `alt_hae_m`).
- Never perform arithmetic between `MSL` and `HAE` without geoid conversion.
- `SRTM .hgt` data in this crate is treated as terrain `MSL`.
- Geoid longitude normalization is internal (`[0, 360)`); SRTM normalization is internal (`[-180, 180)`).
