#!/usr/bin/env bash
set -euo pipefail

MODEL="all"
OUT_DIR="data"
KEEP_ARCHIVE="false"
DOWNLOADED_EGM96="false"
DOWNLOADED_EGM2008="false"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<'EOF'
Download and stage EGM geoid datasets from NGA.

Usage:
  ./scripts/download_geoid_data.sh [--model egm96|egm2008|all] [--out-dir data] [--keep-archive]

Examples:
  ./scripts/download_geoid_data.sh --model egm96
  ./scripts/download_geoid_data.sh --model egm2008 --out-dir data
  ./scripts/download_geoid_data.sh --model all
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

get_file_size() {
  local path="$1"
  if stat -f "%z" "$path" >/dev/null 2>&1; then
    stat -f "%z" "$path"
  else
    stat -c "%s" "$path"
  fi
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
      python3 "${SCRIPT_DIR}/convert_egm96_grd_to_dac.py" \
        --input "$grid_file" \
        --output "${OUT_DIR}/${canonical_name}"
    else
      cp "$grid_file" "${OUT_DIR}/${canonical_name}"
    fi
  else
    cp "$grid_file" "${OUT_DIR}/${canonical_name}"
  fi

  local size
  size="$(get_file_size "${OUT_DIR}/${canonical_name}")"
  echo "Staged ${model} grid at ${OUT_DIR}/${canonical_name} (${size} bytes)"

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
    "WW15MGH.DAC"
  DOWNLOADED_EGM96="true"
}

download_egm2008() {
  download_one \
    "egm2008" \
    "https://earth-info.nga.mil/php/download.php?file=egm-08interpolation" \
    "EGM2008_2_5.DAC"
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
