#!/usr/bin/env bash
set -euo pipefail

tree_output="$(cargo tree -e normal --prefix none)"
runtime_lines="$(echo "$tree_output" | tail -n +2)"
unexpected="$(echo "$runtime_lines" | grep -vE '^byteorder v' || true)"

if [[ -n "$unexpected" ]]; then
  echo "Unexpected runtime dependencies detected:"
  echo "$unexpected"
  exit 1
fi

echo "Runtime dependency check passed (byteorder only)."
