#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_FFI="${ROOT_DIR}/src/ffi.rs"
HEADER="${ROOT_DIR}/include/small_world.h"

rust_exports_file="$(mktemp)"
header_exports_file="$(mktemp)"
trap 'rm -f "${rust_exports_file}" "${header_exports_file}"' EXIT

rg 'pub unsafe extern "C" fn sw_[a-z0-9_]+' -o "${RUST_FFI}" \
  | awk '{print $NF}' \
  | sort -u >"${rust_exports_file}"

rg 'sw_[a-z0-9_]+\(' -o "${HEADER}" \
  | sed 's/(//' \
  | sort -u >"${header_exports_file}"

missing_in_header="$(comm -23 "${rust_exports_file}" "${header_exports_file}" || true)"
missing_in_rust="$(comm -13 "${rust_exports_file}" "${header_exports_file}" || true)"

if [[ -n "${missing_in_header}" || -n "${missing_in_rust}" ]]; then
  echo "C ABI/header drift detected." >&2
  if [[ -n "${missing_in_header}" ]]; then
    echo "Exported in Rust but missing from header:" >&2
    echo "${missing_in_header}" >&2
  fi
  if [[ -n "${missing_in_rust}" ]]; then
    echo "Declared in header but missing from Rust exports:" >&2
    echo "${missing_in_rust}" >&2
  fi
  exit 1
fi

echo "C ABI/header sync check passed ($(wc -l <"${rust_exports_file}" | tr -d ' ') exported functions)."
