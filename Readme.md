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

6. Minimal frame-to-frame conversion example (ENU/NED + AGL/MSL/HAE):

```bash
cargo run --example minimal_frame_conversion
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

### Minimal Conversion Matrices

Assume:

```rust
use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};

let p = GeoPoint::new(lat, lon)?;
let c = AltitudeConverter::new(&geoid, &terrain);
let h = input_m;
```

Converter API (`convert_height_m`):

| from \\ to | `AGL` | `MSL` | `HAE` |
| --- | --- | --- | --- |
| `AGL` | `h` | `c.convert_height_m(p, h, VerticalFrame::Agl, VerticalFrame::Msl)?` | `c.convert_height_m(p, h, VerticalFrame::Agl, VerticalFrame::Hae)?` |
| `MSL` | `c.convert_height_m(p, h, VerticalFrame::Msl, VerticalFrame::Agl)?` | `h` | `c.convert_height_m(p, h, VerticalFrame::Msl, VerticalFrame::Hae)?` |
| `HAE` | `c.convert_height_m(p, h, VerticalFrame::Hae, VerticalFrame::Agl)?` | `c.convert_height_m(p, h, VerticalFrame::Hae, VerticalFrame::Msl)?` | `h` |

WGS84 local-frame API uses explicit per-frame structs (`Lla`, `Ned`, `Enu`) with friendly accessors:

```rust
use small_world::wgs84::{AltType, Enu, Lla, Ned};

let origin = Lla::new(39.0, -77.0, 150.0, AltType::Wgs84);
let enu = Enu::new(15.0, -4.0, 3.0, origin);
let ned = Ned::new(40.0, -8.0, 6.0, origin);

let _ = (enu.e(), enu.n(), enu.u(), enu.origin());
let _ = (ned.n(), ned.e(), ned.d(), ned.origin());
let _ = (origin.lat_deg(), origin.lon_deg(), origin.alt_m(), origin.alt_type());
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

## C/C++ Integration

The crate now exports a stable C ABI for direct C++ use:
- Header: `include/small_world.h`
- Rust artifacts (release build):
  - static library: `target/release/libsmall_world.a`
  - shared library: `target/release/libsmall_world.{so|dylib|dll}` (platform dependent)
- Modern CMake helper: `cmake/SmallWorldRust.cmake`

Build the libraries:

```bash
cargo build --release
```

Minimal C++ compile/link (static, no CMake):

```bash
g++ -std=c++17 -O3 examples/cpp/minimal_conversion.cpp \
  -Iinclude -Ltarget/release -lsmall_world -ldl -lpthread -lm \
  -o /tmp/minimal_conversion
```

### Modern CMake: Recommended Integration

The easiest integration path is the provided helper:
- Include `cmake/SmallWorldRust.cmake`
- Call `small_world_add_rust_library(...)`
- Link your app against `small_world::small_world`

Example (repository checked out as a subdirectory):

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

Example (`FetchContent`):

```cmake
cmake_minimum_required(VERSION 3.21)
project(my_robot_app LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)

include(FetchContent)
FetchContent_Declare(
  small_world
  GIT_REPOSITORY https://github.com/Swarm-Command/small_world.git
  GIT_TAG codex
)
FetchContent_MakeAvailable(small_world)

list(APPEND CMAKE_MODULE_PATH "${small_world_SOURCE_DIR}/cmake")
include(SmallWorldRust)

small_world_add_rust_library(
  TARGET small_world_ffi
  MANIFEST_DIR "${small_world_SOURCE_DIR}"
  PROFILE release
  LINKAGE STATIC
)

add_executable(my_app src/main.cpp)
target_link_libraries(my_app PRIVATE small_world::small_world)
```

Example project in this repo:

```bash
cmake -S examples/cpp -B /tmp/small_world_cpp_build -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/small_world_cpp_build -j
```

See [examples/cpp/README.md](examples/cpp/README.md) for the shortest setup path.

Core ABI design choices:
- Explicit frame enums for all altitude values (`SW_FRAME_AGL`, `SW_FRAME_MSL`, `SW_FRAME_HAE`).
- Opaque converter handle (`SwConverterHandle`) to keep geoid/terrain data loaded and reused.
- Thread-local last-error string (`sw_last_error_message`) for human-readable failure reasons.
- Separate WGS84 local-frame bridge functions (`sw_wgs84_*`) for NED/ENU/LLA conversions.
- Runtime cache telemetry (`sw_converter_terrain_cache_stats`) for performance monitoring.

Typical C++ flow:
1. Call `sw_converter_options_default`.
2. Create a handle with `sw_converter_create`.
3. Convert heights or create absolute LLA points via:
   - `sw_converter_convert_height_m`
   - `sw_converter_lla_wgs84_from_height_m`
4. Perform local/global frame transforms via:
   - `sw_wgs84_enu_to_ned_between_origins`
   - `sw_wgs84_ned_to_lla`
   - `sw_wgs84_lla_to_ned`
5. Destroy handle with `sw_converter_destroy`.

Frame contract for C++ callers:
- `SwLlaWgs84.hae_m` is always WGS84 ellipsoidal height (HAE).
- Terrain query outputs in `SwTerrainReference` are:
  - `ground_msl_m`: terrain orthometric MSL
  - `ground_hae_m`: terrain HAE (`ground_msl_m + geoid_offset_m`)
- Never treat `hae_m` as MSL without explicit conversion.

## Notes

- Geoid longitudes are normalized internally to `[0, 360)`.
- SRTM longitudes are normalized internally to `[-180, 180)`.
- Latitudes must be finite and within `[-90, 90]`.
- Public geoid APIs return typed `Result` errors instead of panicking on invalid input.
