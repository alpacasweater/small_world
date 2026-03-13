# small_world

`small_world` is a lightweight geodesy toolkit for robotics/autonomy that keeps altitude and frame semantics explicit.

## Why teams use it

- Explicit altitude frames (`AGL`, `MSL`, `HAE`) with no implicit guessing.
- Explicit local/global frames (`NED`, `ENU`, `LLA`, `ECEF`) with readable types and accessors.
- Lightweight runtime footprint (`byteorder` only).
- Differentially validated against trusted geospatial tools (PROJ + GDAL).
- C ABI and modern CMake support for C++ deployment.
- Python bindings via PyO3 + Maturin for rapid prototyping and robotics scripting.

## Prerequisites

| Language | Requirement |
|---|---|
| Rust | stable toolchain (`rustup update stable`) |
| Python | Python ≥ 3.9 + `pip install maturin` |
| C/C++ | Rust toolchain only — no GDAL/PROJ needed to build |

## Add to your project

**Rust:**

```toml
[dependencies]
small_world = { path = "../small_world" }
# or:
# small_world = { git = "https://github.com/Swarm-Command/small_world.git", branch = "main" }
```

**Python** — build and install from source:

```bash
pip install maturin
cd python && maturin develop --release
```

```python
from small_world import Lla, Ecef, Ned, Enu, AltitudeConverter, VerticalFrame
```

See [`python/README.md`](python/README.md) for full Python setup, data configuration, and API reference.

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

> Frame-only conversions (`Lla`, `Ecef`, `Ned`, `Enu`) need **no data files**.
> Altitude conversion (`AGL`/`MSL`/`HAE`) requires a geoid file and SRTM tiles.

**Step 1 — data setup** (skip if you only need frame transforms):

```bash
# Geoid (~2 MB, goes to data/WW15MGH.DAC)
./scripts/download_geoid_data.sh --model egm96

# SRTM tiles for your area of interest
./scripts/download_hgt_tiles.sh \
  --lat-min 38.5 --lat-max 39.5 \
  --lon-min -77.6 --lon-max -76.2 \
  --out-dir data/srtm
```

**Step 2 — run a minimal example:**

```bash
# Rust
cargo run --example minimal_frame_conversion

# Python (after: pip install maturin && cd python && maturin develop --release)
python examples/minimal_frame_conversion.py
```

**Step 3 — altitude conversion in code:**

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

Pairwise altitude helpers (all take `(lat_deg, lon_deg, meters) -> meters`):
- `hae_from_msl`, `msl_from_hae`
- `msl_from_agl`, `agl_from_msl`
- `hae_from_agl`, `agl_from_hae`

Reference access:
- `geoid_offset_m(lat_deg, lon_deg)` — geoid separation N in meters
- `ground_msl_m(lat_deg, lon_deg)` — terrain MSL elevation
- `reference(lat_deg, lon_deg)` → `TerrainReference { geoid_offset_m, ground_msl_m, ground_hae_m }`

Local/global transforms:
- `Lla`, `Ecef`, `Ned`, `Enu` types with friendly accessors.
- Checked constructors: `Lla::try_new`, `Ecef::try_new`, `Ned::try_new`, `Enu::try_new`
- Unchecked `new(...)` constructors still exist for low-level/const-style use and do not validate inputs.
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

## C/C++

C ABI surface (from `include/small_world.h`):

| Group | Functions |
|---|---|
| Lifecycle | `sw_converter_options_default`, `sw_converter_create`, `sw_converter_destroy`, `sw_last_error_message` |
| Altitude → scalar | `sw_converter_convert_height_m`, `sw_converter_reference` |
| Altitude → LLA | `sw_converter_lla_wgs84_from_height_m` |
| Altitude → ECEF | `sw_converter_ecef_wgs84_from_height_m`, `sw_converter_height_from_ecef_wgs84_m` |
| Diagnostics | `sw_converter_terrain_cache_stats` |
| LLA ↔ NED | `sw_wgs84_ned_to_lla`, `sw_wgs84_lla_to_ned` |
| LLA ↔ ECEF | `sw_wgs84_lla_to_ecef`, `sw_wgs84_ecef_to_lla` |
| NED ↔ ECEF | `sw_wgs84_ned_to_ecef`, `sw_wgs84_ecef_to_ned` |
| ENU ↔ LLA/NED/ECEF | `sw_wgs84_enu_to_lla`, `sw_wgs84_enu_to_ned_between_origins`, `sw_wgs84_enu_to_ecef`, `sw_wgs84_ecef_to_enu` |

Concurrency:
- Shared converter handles are thread-safe but serialize on an internal mutex.
- For high throughput, use one converter handle per thread.

See `examples/cpp/README.md` for a full CMake integration walkthrough.

## Python

```python
from small_world import Lla, Ecef, Enu, AltitudeConverter, VerticalFrame

# Frame conversions — no data files needed
origin = Lla(39.0, -77.0, 150.0)
ecef   = origin.to_ecef()
enu    = Enu(10.0, 5.0, 2.0, origin)
ned    = enu.to_ned(origin)            # n=5, e=10, d=-2

# Altitude conversion — requires data files
converter = AltitudeConverter("data/WW15MGH.DAC", "data/srtm")
hae_m = converter.convert_height_m(39.0, -77.0, 120.0, VerticalFrame.Agl, VerticalFrame.Hae)
```

Full setup guide, API reference, env vars, and examples: [`python/README.md`](python/README.md)

## More docs

- Production and validation details: [`docs/PRODUCTION.md`](docs/PRODUCTION.md)
- Python bindings: [`python/`](python/) — PyO3 source, type stubs, tests, examples
- C++ and CMake integration walkthrough: [`examples/cpp/README.md`](examples/cpp/README.md)
- Canonical compact Rust example: [`examples/minimal_frame_conversion.rs`](examples/minimal_frame_conversion.rs)
- Performance gate runner: [`scripts/run_perf_smoke.sh`](scripts/run_perf_smoke.sh)
- C ABI/header sync checker: [`scripts/verify_c_header_sync.sh`](scripts/verify_c_header_sync.sh)
