#!/usr/bin/env bash
set -euo pipefail

MODEL="all"
OUT_DIR="data"
KEEP_ARCHIVE="false"
STRICT_CHECKSUM="false"
SHA256_EGM96=""
SHA256_EGM2008=""
HASH_LOG=""
DOWNLOADED_EGM96="false"
DOWNLOADED_EGM2008="false"

usage() {
  cat <<'EOF'
Download and stage EGM geoid datasets from NGA.

Usage:
  ./scripts/download_geoid_data.sh [--model egm96|egm2008|all] [--out-dir data] [--keep-archive]
                                 [--sha256-egm96 <hex>] [--sha256-egm2008 <hex>]
                                 [--strict-checksum] [--sha256-log <path>]

Examples:
  ./scripts/download_geoid_data.sh --model egm96
  ./scripts/download_geoid_data.sh --model egm2008 --out-dir data
  ./scripts/download_geoid_data.sh --model all
  ./scripts/download_geoid_data.sh --model egm96 --sha256-egm96 <expected_hash>
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model)
      MODEL="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --keep-archive)
      KEEP_ARCHIVE="true"
      shift
      ;;
    --sha256-egm96)
      SHA256_EGM96="$2"
      shift 2
      ;;
    --sha256-egm2008)
      SHA256_EGM2008="$2"
      shift 2
      ;;
    --strict-checksum)
      STRICT_CHECKSUM="true"
      shift
      ;;
    --sha256-log)
      HASH_LOG="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required but was not found in PATH" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

if [[ -n "$HASH_LOG" ]]; then
  mkdir -p "$(dirname "$HASH_LOG")"
  : >"$HASH_LOG"
fi

get_file_size() {
  local path="$1"
  if stat -f "%z" "$path" >/dev/null 2>&1; then
    stat -f "%z" "$path"
  else
    stat -c "%s" "$path"
  fi
}

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    echo "sha256sum/shasum is required for checksum verification" >&2
    exit 1
  fi
}

to_lower() {
  echo "$1" | tr '[:upper:]' '[:lower:]'
}

# The NGA egm-96interpolation archive ships only the ASCII WW15MGH.GRD (721 rows x 1441 columns,
# metres; column 1441 duplicates column 1 at the dateline). small_world reads the compact binary
# WW15MGH.DAC layout (721 x 1440 big-endian i16 centimetres), so the grid is converted here. The
# converter is inlined so the script stays a self-contained one-liner when piped from a URL.
convert_grd_to_dac() {
  local input="$1"
  local output="$2"
  python3 - "$input" "$output" <<'PYEOF'
import os
import struct
import sys

ROWS = 721
COLS_INPUT = 1441
COLS_OUTPUT = 1440
HEADER_VALUES = 6

input_path, output_path = sys.argv[1], sys.argv[2]


def token_stream(path):
    with open(path, "r", encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            yield from line.split()


tokens = token_stream(input_path)
header = [float(next(tokens)) for _ in range(HEADER_VALUES)]

with open(output_path, "wb") as out:
    for _row in range(ROWS):
        row_values = [float(next(tokens)) for _ in range(COLS_INPUT)]
        for value_m in row_values[:COLS_OUTPUT]:
            value_cm = int(round(value_m * 100.0))
            if value_cm < -32768 or value_cm > 32767:
                raise RuntimeError(f"Value out of i16 range after cm conversion: {value_m}")
            out.write(struct.pack(">h", value_cm))

expected = ROWS * COLS_OUTPUT * 2
actual = os.path.getsize(output_path)
if actual != expected:
    raise RuntimeError(f"Invalid output size for WW15MGH.DAC: expected {expected}, got {actual}")

print(f"Converted {input_path} -> {output_path} ({actual} bytes)")
PYEOF
}

find_largest_file() {
  local root="$1"
  local best=""
  local best_size=0

  while IFS= read -r -d '' candidate; do
    local size
    size="$(get_file_size "$candidate")"
    if [[ "$size" -gt "$best_size" ]]; then
      best="$candidate"
      best_size="$size"
    fi
  done < <(find "$root" -type f -print0)

  echo "$best"
}

extract_archive() {
  local archive="$1"
  local extract_dir="$2"
  mkdir -p "$extract_dir"

  if command -v unzip >/dev/null 2>&1; then
    if unzip -oq "$archive" -d "$extract_dir" >/dev/null 2>&1; then
      return 0
    fi
  fi

  if command -v tar >/dev/null 2>&1; then
    if tar -xf "$archive" -C "$extract_dir" >/dev/null 2>&1; then
      return 0
    fi
  fi

  if command -v gunzip >/dev/null 2>&1; then
    if gunzip -c "$archive" > "$extract_dir/raw_grid.bin" 2>/dev/null; then
      return 0
    fi
  fi

  cp "$archive" "$extract_dir/raw_grid.bin"
}

select_grid_file() {
  local model="$1"
  local root="$2"
  local candidate=""

  if [[ "$model" == "egm96" ]]; then
    candidate="$(find "$root" -type f -iname 'WW15MGH.GRD' | head -n 1 || true)"
    if [[ -z "$candidate" ]]; then
      candidate="$(find "$root" -type f -iname 'WW15MGH.DAC' | head -n 1 || true)"
    fi
    if [[ -z "$candidate" ]]; then
      candidate="$(find "$root" -type f \( -iname '*WW15MGH*' -o -iname '*egm96*' \) | head -n 1 || true)"
    fi
  else
    candidate="$(find "$root" -type f -iname 'Und_min2.5x2.5_egm2008_isw=82_WGS84_TideFree_SE' | head -n 1 || true)"
    if [[ -z "$candidate" ]]; then
      candidate="$(find "$root" -type f \( -iname '*egm2008*' -o -iname '*2.5*' -o -iname '*und_min*' \) | head -n 1 || true)"
    fi
  fi

  if [[ -z "$candidate" ]]; then
    candidate="$(find_largest_file "$root")"
  fi

  if [[ -z "$candidate" ]]; then
    echo "Failed to find grid file for ${model} in extracted archive content" >&2
    return 1
  fi

  echo "$candidate"
}

download_one() {
  local model="$1"
  local url="$2"
  local canonical_name="$3"
  local expected_sha256="$4"

  local tmp_dir
  tmp_dir="$(mktemp -d)"
  local archive="$tmp_dir/${model}.download"
  local extract_dir="$tmp_dir/extracted"

  echo "Downloading ${model} dataset from ${url}"
  curl -fL --retry 5 --retry-delay 2 --connect-timeout 20 -o "$archive" "$url"

  extract_archive "$archive" "$extract_dir"
  local grid_file
  grid_file="$(select_grid_file "$model" "$extract_dir")"

  if [[ "$model" == "egm96" ]]; then
    if file "$grid_file" | grep -q "ASCII text"; then
      if ! command -v python3 >/dev/null 2>&1; then
        echo "python3 is required to convert WW15MGH.GRD to WW15MGH.DAC" >&2
        exit 1
      fi
      convert_grd_to_dac "$grid_file" "${OUT_DIR}/${canonical_name}"
    else
      cp "$grid_file" "${OUT_DIR}/${canonical_name}"
    fi
  else
    cp "$grid_file" "${OUT_DIR}/${canonical_name}"
  fi

  local size
  size="$(get_file_size "${OUT_DIR}/${canonical_name}")"
  echo "Staged ${model} grid at ${OUT_DIR}/${canonical_name} (${size} bytes)"

  local actual_sha256
  actual_sha256="$(sha256_file "${OUT_DIR}/${canonical_name}")"
  echo "SHA256 ${canonical_name}: ${actual_sha256}"

  if [[ "$STRICT_CHECKSUM" == "true" && -z "$expected_sha256" ]]; then
    echo "strict checksum mode enabled but no expected checksum was provided for ${model}" >&2
    exit 1
  fi
  if [[ -n "$expected_sha256" ]]; then
    if [[ "$(to_lower "$actual_sha256")" != "$(to_lower "$expected_sha256")" ]]; then
      echo "checksum mismatch for ${canonical_name}" >&2
      echo "  expected: ${expected_sha256}" >&2
      echo "  actual:   ${actual_sha256}" >&2
      exit 1
    fi
  fi
  if [[ -n "$HASH_LOG" ]]; then
    echo "${actual_sha256}  ${canonical_name}" >>"$HASH_LOG"
  fi

  if [[ "$KEEP_ARCHIVE" == "true" ]]; then
    cp "$archive" "${OUT_DIR}/${model}.download"
    echo "Saved archive as ${OUT_DIR}/${model}.download"
  fi

  rm -rf "$tmp_dir"
}

download_egm96() {
  download_one \
    "egm96" \
    "https://earth-info.nga.mil/php/download.php?file=egm-96interpolation" \
    "WW15MGH.DAC" \
    "$SHA256_EGM96"
  DOWNLOADED_EGM96="true"
}

download_egm2008() {
  download_one \
    "egm2008" \
    "https://earth-info.nga.mil/php/download.php?file=egm-08interpolation" \
    "EGM2008_2_5.DAC" \
    "$SHA256_EGM2008"
  DOWNLOADED_EGM2008="true"
}

case "$MODEL" in
  egm96)
    download_egm96
    ;;
  egm2008)
    download_egm2008
    ;;
  all)
    download_egm96
    download_egm2008
    ;;
  *)
    echo "Unsupported model '${MODEL}'. Use egm96, egm2008, or all." >&2
    exit 1
    ;;
esac

echo
echo "Done."
if [[ "$DOWNLOADED_EGM96" == "true" ]]; then
  echo "EGM96 file:   ${OUT_DIR}/WW15MGH.DAC"
fi
if [[ "$DOWNLOADED_EGM2008" == "true" ]]; then
  echo "EGM2008 file: ${OUT_DIR}/EGM2008_2_5.DAC"
fi
