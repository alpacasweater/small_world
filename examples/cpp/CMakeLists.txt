cmake_minimum_required(VERSION 3.21)
project(small_world_cpp_example LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_LIST_DIR}/../../cmake")
include(SmallWorldRust)

small_world_add_rust_library(
  TARGET small_world_ffi
  MANIFEST_DIR "${CMAKE_CURRENT_LIST_DIR}/../.."
  PROFILE release
  LINKAGE STATIC
)

add_executable(minimal_conversion minimal_conversion.cpp)
target_link_libraries(minimal_conversion PRIVATE small_world::small_world)
