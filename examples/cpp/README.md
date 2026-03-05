# small_world C++ Example (Modern CMake)

This directory shows the recommended CMake integration path for `small_world`:
- `CMakeLists.txt` uses `cmake/SmallWorldRust.cmake`
- `minimal_conversion.cpp` exercises the C ABI (`include/small_world.h`)

## Build

From repository root:

```bash
cmake -S examples/cpp -B /tmp/small_world_cpp_build -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/small_world_cpp_build -j
```

Binary:

```bash
/tmp/small_world_cpp_build/minimal_conversion
```

Runtime smoke test (dataset-free, recommended for CI):

```bash
ctest --test-dir /tmp/small_world_cpp_build -C Release --output-on-failure
```

## How It Works

`small_world_add_rust_library(...)`:
1. Runs `cargo build` for this repo.
2. Creates imported CMake target `small_world::small_world`.
3. Exposes `include/small_world.h` automatically as an interface include path.

You can link any of your own C++ targets directly against `small_world::small_world`.

## Concurrency guidance

- `sw_converter_*` calls are thread-safe.
- A single shared `SwConverterHandle*` is safe but serialized internally.
- For high-throughput workloads, create one converter handle per worker thread.
