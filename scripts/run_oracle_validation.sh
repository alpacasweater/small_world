#!/usr/bin/env bash
set -euo pipefail

if ! command -v cct >/dev/null 2>&1; then
  echo "ERROR: cct (PROJ) not found on PATH." >&2
  echo "Install PROJ (e.g., apt-get install proj-bin, brew install proj)." >&2
  exit 1
fi

SMALL_WORLD_REQUIRE_PROJ=1 cargo test --test oracle_proj -- --nocapture
