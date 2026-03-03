#!/usr/bin/env python3
"""Convert NGA WW15MGH.GRD ASCII grid into small_world WW15MGH.DAC binary.

Output layout:
- 721 rows
- 1440 columns (longitude 0.0..359.75, 0.25 deg step)
- big-endian i16 values in centimeters
"""

from __future__ import annotations

import argparse
import struct
from pathlib import Path
from typing import Iterator

ROWS = 721
COLS_INPUT = 1441
COLS_OUTPUT = 1440
HEADER_VALUES = 6


def token_stream(path: Path) -> Iterator[str]:
    with path.open("r", encoding="utf-8", errors="ignore") as handle:
        for line in handle:
            for token in line.split():
                yield token


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, help="Path to WW15MGH.GRD")
    parser.add_argument("--output", required=True, help="Path to WW15MGH.DAC")
    args = parser.parse_args()

    input_path = Path(args.input)
    output_path = Path(args.output)

    tokens = token_stream(input_path)
    header = [float(next(tokens)) for _ in range(HEADER_VALUES)]

    if len(header) != HEADER_VALUES:
        raise RuntimeError("Invalid WW15MGH.GRD header")

    with output_path.open("wb") as out:
        for _row in range(ROWS):
            row_values = [float(next(tokens)) for _ in range(COLS_INPUT)]
            for value_m in row_values[:COLS_OUTPUT]:
                value_cm = int(round(value_m * 100.0))
                if value_cm < -32768 or value_cm > 32767:
                    raise RuntimeError(f"Value out of i16 range after cm conversion: {value_m}")
                out.write(struct.pack(">h", value_cm))

    expected_bytes = ROWS * COLS_OUTPUT * 2
    actual_bytes = output_path.stat().st_size
    if actual_bytes != expected_bytes:
        raise RuntimeError(
            f"Invalid output size for WW15MGH.DAC: expected {expected_bytes}, got {actual_bytes}"
        )

    print(f"Converted {input_path} -> {output_path} ({actual_bytes} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

