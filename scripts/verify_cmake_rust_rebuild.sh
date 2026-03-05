#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUILD_DIR="${BUILD_DIR:-$(mktemp -d /tmp/small_world_cmake_rebuild.XXXXXX)}"
TARGET_NAME="small_world_ffi"
STAMP_PATH="${BUILD_DIR}/small_world_rust/${TARGET_NAME}_release_static.stamp"
TOUCH_FILE="${ROOT_DIR}/src/lib.rs"

mtime_seconds() {
  local path="$1"
  if stat -f "%m" "$path" >/dev/null 2>&1; then
    stat -f "%m" "$path"
  else
    stat -c "%Y" "$path"
  fi
}

echo "Configuring CMake example at ${BUILD_DIR}"
cmake -S "${ROOT_DIR}/examples/cpp" -B "${BUILD_DIR}" -DCMAKE_BUILD_TYPE=Release >/dev/null

echo "Initial build"
cmake --build "${BUILD_DIR}" -j >/dev/null

if [[ ! -f "${STAMP_PATH}" ]]; then
  echo "ERROR: expected stamp file not found: ${STAMP_PATH}" >&2
  exit 1
fi

before_stamp_mtime="$(mtime_seconds "${STAMP_PATH}")"

# Ensure timestamp granularity is crossed before touching a source file.
sleep 1

touch "${TOUCH_FILE}"

echo "Rebuild after touching Rust source"
cmake --build "${BUILD_DIR}" -j >/dev/null

after_stamp_mtime="$(mtime_seconds "${STAMP_PATH}")"

if (( after_stamp_mtime <= before_stamp_mtime )); then
  echo "ERROR: CMake did not refresh Rust build stamp after source touch." >&2
  echo "before=${before_stamp_mtime} after=${after_stamp_mtime}" >&2
  exit 1
fi

echo "CMake rebuild verification passed (stamp advanced ${before_stamp_mtime} -> ${after_stamp_mtime})."
