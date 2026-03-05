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

echo "PROJ version:"
cct --version
echo "GDAL version:"
gdalinfo --version

echo "Running trusted external altitude oracle tests (PROJ + GDAL)..."
SMALL_WORLD_REQUIRE_EXTERNAL_ORACLES=1 cargo test --test oracle_altitude_external -- --nocapture

echo "Running PROJ differential oracle tests for local/global frames..."
SMALL_WORLD_REQUIRE_PROJ=1 cargo test --test oracle_proj -- --nocapture

echo "Running analytic altitude invariants (supplemental, non-oracle)..."
cargo test --test oracle_altitude -- --nocapture
