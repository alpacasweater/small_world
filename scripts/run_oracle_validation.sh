#!/usr/bin/env bash
set -euo pipefail

if ! command -v cct >/dev/null 2>&1; then
  echo "ERROR: cct (PROJ) not found on PATH." >&2
  echo "Install PROJ (e.g., apt-get install proj-bin, brew install proj)." >&2
  exit 1
fi

if ! command -v gdallocationinfo >/dev/null 2>&1; then
  echo "ERROR: gdallocationinfo (GDAL) not found on PATH." >&2
  echo "Install GDAL (e.g., apt-get install gdal-bin, brew install gdal)." >&2
  exit 1
fi

if [[ ! -f data/WW15MGH.DAC ]]; then
  echo "ERROR: required geoid dataset missing: data/WW15MGH.DAC" >&2
  echo "Run ./scripts/download_geoid_data.sh --model egm96" >&2
  exit 1
fi

if [[ ! -f data/srtm/N39W077.hgt ]]; then
  echo "Real-terrain tile missing. Downloading data/srtm/N39W077.hgt for oracle validation..."
  ./scripts/download_hgt_tiles.sh \
    --lat-min 39 --lat-max 40 \
    --lon-min -77 --lon-max -76 \
    --out-dir data/srtm \
    --sha256-manifest data/oracle_srtm_sha256.txt \
    --strict-checksum \
    --max-size 100MB
fi

if [[ ! -f data/srtm/N39W077.hgt ]]; then
  echo "ERROR: failed to stage required terrain tile data/srtm/N39W077.hgt" >&2
  exit 1
fi

# Verify deterministic tile integrity even when tile pre-exists.
./scripts/download_hgt_tiles.sh \
  --lat-min 39 --lat-max 40 \
  --lon-min -77 --lon-max -76 \
  --out-dir data/srtm \
  --sha256-manifest data/oracle_srtm_sha256.txt \
  --strict-checksum \
  --dry-run

echo "PROJ version:"
cct --version
echo "GDAL version:"
gdalinfo --version

echo "Running trusted external altitude oracle tests (PROJ + GDAL)..."
SMALL_WORLD_REQUIRE_EXTERNAL_ORACLES=1 cargo test --test oracle_altitude_external -- --nocapture

echo "Running PROJ differential oracle tests for local/global frames..."
SMALL_WORLD_REQUIRE_PROJ=1 cargo test --test oracle_proj -- --nocapture

echo "Running real-terrain oracle alignment test (GDAL + PROJ + local HGT)..."
SMALL_WORLD_REQUIRE_EXTERNAL_ORACLES=1 cargo test --test oracle_real_terrain -- --nocapture

echo "Running analytic altitude invariants (supplemental, non-oracle)..."
cargo test --test oracle_altitude -- --nocapture
