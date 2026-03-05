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

if [[ ! -f data/oracle_srtm_sha256.txt ]]; then
  echo "ERROR: missing checksum manifest: data/oracle_srtm_sha256.txt" >&2
  exit 1
fi

stage_required_tile() {
  local lat_min="$1"
  local lat_max="$2"
  local lon_min="$3"
  local lon_max="$4"
  local tile_name="$5"

  echo "Staging oracle tile ${tile_name}..."
  ./scripts/download_hgt_tiles.sh \
    --lat-min "${lat_min}" --lat-max "${lat_max}" \
    --lon-min "${lon_min}" --lon-max "${lon_max}" \
    --out-dir data/srtm \
    --sha256-manifest data/oracle_srtm_sha256.txt \
    --strict-checksum \
    --max-size 1GB

  if [[ ! -f "data/srtm/${tile_name}" ]]; then
    echo "ERROR: failed to stage required terrain tile data/srtm/${tile_name}" >&2
    exit 1
  fi
}

# Multi-region real-terrain oracle corpus.
stage_required_tile 39 40 -77 -76 N39W077.hgt   # United States (Maryland)
stage_required_tile 35 36 139 140 N35E139.hgt   # Japan (Tokyo)
stage_required_tile 37 38 127 128 N37E127.hgt   # South Korea (Seoul)
stage_required_tile -33 -32 151 152 S33E151.hgt # Australia (Sydney)
stage_required_tile -22 -21 -43 -42 S22W043.hgt # Brazil (Rio)
stage_required_tile 51 52 0 1 N51E000.hgt       # United Kingdom (London)
stage_required_tile 27 28 86 87 N27E086.hgt     # Nepal (Himalaya)

echo "PROJ version:"
cct --version
echo "GDAL version:"
gdalinfo --version

echo "Running trusted external altitude oracle tests (PROJ + GDAL)..."
SMALL_WORLD_REQUIRE_EXTERNAL_ORACLES=1 cargo test --test oracle_altitude_external -- --nocapture

echo "Running PROJ differential oracle tests for local/global frames..."
SMALL_WORLD_REQUIRE_PROJ=1 cargo test --test oracle_proj -- --nocapture

echo "Running real-terrain oracle alignment test (GDAL + PROJ + multi-region HGT corpus)..."
SMALL_WORLD_REQUIRE_EXTERNAL_ORACLES=1 cargo test --test oracle_real_terrain -- --nocapture

echo "Running analytic altitude invariants (supplemental, non-oracle)..."
cargo test --test oracle_altitude -- --nocapture
