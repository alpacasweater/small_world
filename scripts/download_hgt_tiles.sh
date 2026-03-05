#!/usr/bin/env bash
set -euo pipefail

OUT_DIR="data/srtm"
LAT_MIN=""
LAT_MAX=""
LON_MIN=""
LON_MAX=""
MAX_BYTES=$((5 * 1024 * 1024 * 1024))
BASE_URL="https://s3.amazonaws.com/elevation-tiles-prod/skadi"
DRY_RUN="false"
FORCE="false"

usage() {
  cat <<'EOF'
Download SRTM .hgt tiles for a bounding box with a hard storage cap (default 5 GiB).

Source:
  https://s3.amazonaws.com/elevation-tiles-prod/skadi
  (public SRTM HGT tile archive, delivered as .hgt.gz files)

Usage:
  ./scripts/download_hgt_tiles.sh \
    --lat-min <deg> --lat-max <deg> \
    --lon-min <deg> --lon-max <deg> \
    [--out-dir data/srtm] \
    [--max-size 5GB] \
    [--dry-run] [--force]

Examples:
  # Washington DC area
  ./scripts/download_hgt_tiles.sh \
    --lat-min 38.5 --lat-max 39.5 --lon-min -77.6 --lon-max -76.2

  # Anti-meridian crossing example
  ./scripts/download_hgt_tiles.sh \
    --lat-min -18 --lat-max -16 --lon-min 179 --lon-max -178

  # Dry run only (no downloads)
  ./scripts/download_hgt_tiles.sh \
    --lat-min 34 --lat-max 35 --lon-min -119 --lon-max -117 --dry-run
EOF
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

is_finite_number() {
  local value="$1"
  awk -v x="$value" 'BEGIN { if (x+0==x && x!="inf" && x!="-inf" && x!="nan" && x!="NaN") exit 0; exit 1 }'
}

floor_float() {
  awk -v x="$1" 'BEGIN { i=int(x); print (x<i) ? i-1 : i }'
}

ceil_float() {
  awk -v x="$1" 'BEGIN { i=int(x); print (x>i) ? i+1 : i }'
}

normalize_lon_deg() {
  awk -v x="$1" 'BEGIN {
    y = x
    while (y < -180.0) y += 360.0
    while (y >= 180.0) y -= 360.0
    printf "%.12f\n", y
  }'
}

normalize_lon_floor() {
  local value="$1"
  while (( value < -180 )); do
    value=$((value + 360))
  done
  while (( value > 179 )); do
    value=$((value - 360))
  done
  echo "$value"
}

file_size() {
  local path="$1"
  if stat -f "%z" "$path" >/dev/null 2>&1; then
    stat -f "%z" "$path"
  else
    stat -c "%s" "$path"
  fi
}

human_bytes() {
  awk -v b="$1" 'BEGIN {
    split("B KiB MiB GiB TiB", u, " ")
    i=1
    while (b >= 1024 && i < 5) { b /= 1024; i++ }
    printf "%.2f %s", b, u[i]
  }'
}

parse_size_bytes() {
  local raw="$1"
  local s num unit mult
  s="$(echo "$raw" | tr -d '[:space:]' | tr '[:lower:]' '[:upper:]')"
  if [[ ! "$s" =~ ^([0-9]+([.][0-9]+)?)(B|KB|KIB|MB|MIB|GB|GIB|TB|TIB)?$ ]]; then
    return 1
  fi
  num="${BASH_REMATCH[1]}"
  unit="${BASH_REMATCH[3]}"
  mult=1
  case "$unit" in
    ""|B) mult=1 ;;
    KB|KIB) mult=1024 ;;
    MB|MIB) mult=$((1024 * 1024)) ;;
    GB|GIB) mult=$((1024 * 1024 * 1024)) ;;
    TB|TIB) mult=$((1024 * 1024 * 1024 * 1024)) ;;
    *) return 1 ;;
  esac
  awk -v n="$num" -v m="$mult" 'BEGIN { printf "%.0f", n * m }'
}

download_tile_gz() {
  local url="$1"
  local output_gz="$2"
  curl -fsSL --retry 4 --retry-delay 1 --connect-timeout 20 -o "$output_gz" "$url"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --lat-min)
      LAT_MIN="$2"
      shift 2
      ;;
    --lat-max)
      LAT_MAX="$2"
      shift 2
      ;;
    --lon-min)
      LON_MIN="$2"
      shift 2
      ;;
    --lon-max)
      LON_MAX="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --max-size)
      MAX_BYTES="$(parse_size_bytes "$2")" || fail "invalid --max-size value: $2"
      shift 2
      ;;
    --base-url)
      BASE_URL="$2"
      shift 2
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    --force)
      FORCE="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ -n "$LAT_MIN" && -n "$LAT_MAX" && -n "$LON_MIN" && -n "$LON_MAX" ]] || {
  usage
  fail "missing required bbox arguments"
}

is_finite_number "$LAT_MIN" || fail "--lat-min must be finite"
is_finite_number "$LAT_MAX" || fail "--lat-max must be finite"
is_finite_number "$LON_MIN" || fail "--lon-min must be finite"
is_finite_number "$LON_MAX" || fail "--lon-max must be finite"

awk -v a="$LAT_MIN" -v b="$LAT_MAX" 'BEGIN { if (a >= b) exit 1 }' || fail "--lat-min must be < --lat-max"
awk -v a="$LAT_MIN" 'BEGIN { if (a < -90 || a > 90) exit 1 }' || fail "--lat-min out of bounds [-90, 90]"
awk -v a="$LAT_MAX" 'BEGIN { if (a < -90 || a > 90) exit 1 }' || fail "--lat-max out of bounds [-90, 90]"

have_cmd curl || fail "curl is required"
have_cmd gzip || fail "gzip is required"
have_cmd gunzip || fail "gunzip is required"

mkdir -p "$OUT_DIR"

tile_list_file="$(mktemp)"
tile_list_sorted="$(mktemp)"
trap 'rm -f "$tile_list_file" "$tile_list_sorted"' EXIT

lat_start=$(floor_float "$LAT_MIN")
lat_end_exclusive=$(ceil_float "$LAT_MAX")
lat_end=$((lat_end_exclusive - 1))
(( lat_start < -90 )) && lat_start=-90
(( lat_end > 89 )) && lat_end=89
(( lat_end >= lat_start )) || fail "bbox produces no latitude tiles"

lon_span_abs=$(awk -v a="$LON_MIN" -v b="$LON_MAX" 'BEGIN { d=b-a; if (d<0) d=-d; printf "%.12f", d }')
full_lon="false"
awk -v s="$lon_span_abs" 'BEGIN { if (s >= 360.0) exit 0; exit 1 }' && full_lon="true"

lon_min_norm=$(normalize_lon_deg "$LON_MIN")
lon_max_norm=$(normalize_lon_deg "$LON_MAX")

declare -a lon_ranges=()
if [[ "$full_lon" == "true" ]]; then
  lon_ranges+=("-180:179")
else
  if awk -v a="$lon_min_norm" -v b="$lon_max_norm" 'BEGIN { if (a <= b) exit 0; exit 1 }'; then
    ls=$(floor_float "$lon_min_norm")
    le=$(( $(ceil_float "$lon_max_norm") - 1 ))
    lon_ranges+=("${ls}:${le}")
  else
    ls1=$(floor_float "$lon_min_norm")
    lon_ranges+=("${ls1}:179")
    le2=$(( $(ceil_float "$lon_max_norm") - 1 ))
    lon_ranges+=("-180:${le2}")
  fi
fi

for ((lat_floor = lat_start; lat_floor <= lat_end; lat_floor++)); do
  for range in "${lon_ranges[@]}"; do
    IFS=":" read -r lon_start lon_end <<<"$range"
    for ((lon_floor = lon_start; lon_floor <= lon_end; lon_floor++)); do
      lon_norm=$(normalize_lon_floor "$lon_floor")
      lat_abs=$(printf "%02d" "${lat_floor#-}")
      lon_abs=$(printf "%03d" "${lon_norm#-}")
      if (( lat_floor >= 0 )); then
        lat_prefix="N"
      else
        lat_prefix="S"
      fi
      if (( lon_norm >= 0 )); then
        lon_prefix="E"
      else
        lon_prefix="W"
      fi
      tile="${lat_prefix}${lat_abs}${lon_prefix}${lon_abs}.hgt"
      echo "$tile" >>"$tile_list_file"
    done
  done
done

sort -u "$tile_list_file" >"$tile_list_sorted"
selected_tiles="$(wc -l <"$tile_list_sorted" | tr -d ' ')"
if [[ "$selected_tiles" == "0" ]]; then
  fail "no tiles selected for the provided bbox"
fi

current_bytes=0
while IFS= read -r -d '' existing; do
  current_bytes=$((current_bytes + $(file_size "$existing")))
done < <(find "$OUT_DIR" -type f -name '*.hgt' -print0)

echo "Selected tiles: ${selected_tiles}"
echo "Output directory: $OUT_DIR"
echo "Current .hgt usage: $(human_bytes "$current_bytes")"
echo "Max .hgt usage cap: $(human_bytes "$MAX_BYTES")"
echo

downloaded=0
skipped_existing=0
skipped_missing=0
skipped_budget=0
errors=0

while IFS= read -r tile; do
  local_path="${OUT_DIR}/${tile}"
  lat_band="${tile:0:3}"   # e.g., N39
  remote_url="${BASE_URL}/${lat_band}/${tile}.gz"

  if [[ -f "$local_path" && "$FORCE" != "true" ]]; then
    skipped_existing=$((skipped_existing + 1))
    continue
  fi

  if [[ "$DRY_RUN" == "true" ]]; then
    echo "[dry-run] would fetch ${remote_url} -> ${local_path}"
    continue
  fi

  tmp_gz="$(mktemp)"
  tmp_hgt="${local_path}.tmp"
  rm -f "$tmp_hgt"

  if ! download_tile_gz "$remote_url" "$tmp_gz"; then
    skipped_missing=$((skipped_missing + 1))
    rm -f "$tmp_gz"
    continue
  fi

  uncompressed_size="$(gzip -l "$tmp_gz" | awk 'NR==2 {print $2}')"
  if [[ -z "$uncompressed_size" || "$uncompressed_size" == "0" ]]; then
    # Fallback for tools that do not populate gzip -l size.
    uncompressed_size="$(gunzip -c "$tmp_gz" | wc -c | tr -d ' ')"
  fi

  if (( current_bytes + uncompressed_size > MAX_BYTES )); then
    echo "Stopping at tile ${tile}: budget would exceed cap ($(human_bytes "$MAX_BYTES"))."
    skipped_budget=$((skipped_budget + 1))
    rm -f "$tmp_gz"
    break
  fi

  if ! gunzip -c "$tmp_gz" >"$tmp_hgt"; then
    echo "Failed to decompress $remote_url" >&2
    errors=$((errors + 1))
    rm -f "$tmp_gz" "$tmp_hgt"
    continue
  fi
  rm -f "$tmp_gz"

  mv "$tmp_hgt" "$local_path"
  actual_size=$(file_size "$local_path")
  current_bytes=$((current_bytes + actual_size))
  downloaded=$((downloaded + 1))
  echo "Downloaded ${tile} ($(human_bytes "$actual_size")), total=$(human_bytes "$current_bytes")"
done <"$tile_list_sorted"

echo
echo "Summary:"
echo "  Downloaded:       $downloaded"
echo "  Skipped existing: $skipped_existing"
echo "  Missing remote:   $skipped_missing"
echo "  Skipped budget:   $skipped_budget"
echo "  Errors:           $errors"
echo "  Final .hgt usage: $(human_bytes "$current_bytes")"

if [[ "$DRY_RUN" == "true" ]]; then
  echo
  echo "Dry run complete. No files downloaded."
fi

if (( errors > 0 )); then
  exit 1
fi
