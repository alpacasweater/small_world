# small_world

[![crates.io](https://img.shields.io/crates/v/small_world.svg)](https://crates.io/crates/small_world)
[![docs.rs](https://img.shields.io/docsrs/small_world)](https://docs.rs/small_world)
[![CI](https://github.com/alpacasweater/small_world/actions/workflows/ci.yml/badge.svg)](https://github.com/alpacasweater/small_world/actions)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

`small_world` is a lightweight geodesy toolkit for robotics/autonomy that keeps altitude and frame semantics explicit.

## Why teams use it

- Explicit altitude frames (`AGL`, `MSL`, `HAE`) with no implicit guessing.
- Explicit local/global frames (`NED`, `ENU`, `LLA`, `ECEF`) with readable types and accessors.
- Lightweight runtime footprint (`byteorder` only).
- Differentially validated against trusted geospatial tools (PROJ + GDAL).
- C ABI and modern CMake support for C++ deployment.

## Add to your project

```toml
[dependencies]
small_world = "0.1"
```

To do altitude conversion without managing any data files, enable the embedded EGM96 geoid
(~2 MiB added to the binary; see [Data attribution](#data-attribution)):

```toml
[dependencies]
small_world = { version = "0.1", features = ["embedded-egm96"] }
```

C/C++ consumers: build from source (`cargo build --release` produces the static and shared
libraries; the header is [`include/small_world.h`](include/small_world.h)) — see
[`examples/cpp/README.md`](examples/cpp/README.md) for the CMake walkthrough.

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

### Which MSL? (EGM96 vs EGM2008)

"MSL" is not a single datum: an orthometric height is relative to a specific **geoid model**,
and the two models this crate supports disagree by decimeters over much of the Earth (locally
approaching a meter). At RTK/centimeter accuracy that disagreement *is* your error budget — so
the model is part of the type, not a comment:

- `VerticalFrame::Msl(EgmModel)` names its model. Conversions **verify the tag against the
  converter's geoid** and fail with `GeoidModelMismatch` rather than silently reinterpreting a
  value — a mistake worth 10–100× RTK measurement noise refuses to compile into an answer.
- `AGL` conversions verify the **terrain dataset's vertical datum** against the geoid
  (`TerrainDatumMismatch` otherwise): SRTM heights are EGM96-referenced, so pairing them with
  an EGM2008 geoid would mix datums inside a single sum.
- The untagged pairwise helpers (`hae_from_msl`, …) operate in the converter's own model —
  `converter.geoid_model()` reports which that is.
- If a data source doesn't document its MSL model (many GNSS receivers use built-in, often
  coarse, EGM96-derived tables), resolve that before trusting centimeters — or sidestep it by
  exchanging heights as **HAE**, which is model-free.

## Quick start

> Frame-only conversions (`Lla`, `Ecef`, `Ned`, `Enu`) need **no data files**.
> Altitude conversion (`AGL`/`MSL`/`HAE`) needs a geoid; `AGL` additionally needs SRTM tiles.

**Step 1 — pick a geoid:**

| | Setup | Use |
|---|---|---|
| **EGM96, embedded** (recommended start) | none — just the `embedded-egm96` feature | `EGM96::embedded()?` |
| **EGM96, downloaded** (~2 MB) | one command, below | `EGM96::new("data/WW15MGH.DAC".as_ref())?` |
| **EGM2008, downloaded** (~142 MB, 2.5′ resolution) | one command, below | `EGM2008::new("data/EGM2008_2_5.DAC".as_ref())?` |

The download one-liners (work anywhere; no repo checkout needed):

```bash
curl -fsSL https://raw.githubusercontent.com/alpacasweater/small_world/main/scripts/download_geoid_data.sh | bash -s -- --model egm96
curl -fsSL https://raw.githubusercontent.com/alpacasweater/small_world/main/scripts/download_geoid_data.sh | bash -s -- --model egm2008
```

(In a checkout: `./scripts/download_geoid_data.sh --model egm96|egm2008`. Prefer to read
before running? The script lives at [`scripts/download_geoid_data.sh`](scripts/download_geoid_data.sh)
and supports `--sha256-egm96`/`--sha256-egm2008` for checksum pinning.)

You can't get this wrong quietly: constructing a geoid from a missing file returns an error
whose message contains the exact command above.

**Also for `AGL`: SRTM tiles for your area of interest:**

```bash
./scripts/download_hgt_tiles.sh \
  --lat-min 38.5 --lat-max 39.5 \
  --lon-min -77.6 --lon-max -76.2 \
  --out-dir data/srtm
```

**Step 2 — run a minimal example:**

```bash
cargo run --example minimal_frame_conversion
```

**Step 3 — altitude conversion in code:**

```rust
use small_world::altitude::{AltitudeConverter, GeoPoint, VerticalFrame};
use small_world::geoid::EGM96;
use small_world::terrain::SrtmDataset;

let geoid = EGM96::embedded()?; // zero-setup (embedded-egm96 feature)
// or: EGM96::new(std::path::Path::new("data/WW15MGH.DAC"))?
// or: EGM2008::new(std::path::Path::new("data/EGM2008_2_5.DAC"))? — higher resolution
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

See [`examples/cpp/README.md`](examples/cpp/README.md) for a full CMake integration walkthrough.

## More docs

- Production and validation details: [`docs/PRODUCTION.md`](docs/PRODUCTION.md)
- C++ and CMake integration walkthrough: [`examples/cpp/README.md`](examples/cpp/README.md)
- Canonical compact Rust example: [`examples/minimal_frame_conversion.rs`](examples/minimal_frame_conversion.rs)
- Performance gate runner: [`scripts/run_perf_smoke.sh`](scripts/run_perf_smoke.sh)
- C ABI/header sync checker: [`scripts/verify_c_header_sync.sh`](scripts/verify_c_header_sync.sh)

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed
as above, without any additional terms or conditions.

## Data attribution

The bundled EGM96 geoid grid (`data/WW15MGH.DAC`) is a U.S. Government work produced by
NIMA (now NGA) and NASA GSFC, redistributed unmodified; it is data, not code, and is not
covered by the licenses above. Details and sources: [NOTICE](NOTICE).
