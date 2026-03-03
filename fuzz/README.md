# Fuzzing

This project keeps fuzzing dependencies isolated in `fuzz/` so end-user runtime dependencies remain minimal.
Fuzzing uses nightly only; normal library usage remains on stable Rust.

## Setup

Install `cargo-fuzz` once:

```bash
cargo install cargo-fuzz
```

Install nightly toolchain for sanitizer-based fuzzing:

```bash
rustup toolchain install nightly
```

## Targets

- `terrain_hgt`: fuzzes SRTM `.hgt` parsing/interpolation paths.
- `egm_grid`: fuzzes EGM dataset loading and offset query paths.

## Run

From the `fuzz/` directory:

```bash
cargo +nightly fuzz run terrain_hgt
cargo +nightly fuzz run egm_grid
```

Examples with time limits:

```bash
cargo +nightly fuzz run terrain_hgt -- -max_total_time=300
cargo +nightly fuzz run egm_grid -- -max_total_time=300
```
