include_guard(GLOBAL)

include(CMakeParseArguments)
find_package(Threads REQUIRED)

function(small_world_add_rust_library)
  set(options)
  set(one_value_args TARGET MANIFEST_DIR PROFILE LINKAGE CARGO_TARGET_DIR)
  cmake_parse_arguments(SW "${options}" "${one_value_args}" "" ${ARGN})

  if(NOT SW_TARGET)
    message(FATAL_ERROR "small_world_add_rust_library requires TARGET")
  endif()

  if(NOT SW_MANIFEST_DIR)
    set(SW_MANIFEST_DIR "${CMAKE_CURRENT_LIST_DIR}/..")
  endif()
  if(NOT SW_PROFILE)
    set(SW_PROFILE release)
  endif()
  if(NOT SW_LINKAGE)
    set(SW_LINKAGE STATIC)
  endif()
  if(NOT SW_CARGO_TARGET_DIR)
    set(SW_CARGO_TARGET_DIR "${CMAKE_BINARY_DIR}/small_world_rust")
  endif()

  find_program(SW_CARGO_EXECUTABLE cargo REQUIRED)

  set(_manifest_path "${SW_MANIFEST_DIR}/Cargo.toml")
  if(NOT EXISTS "${_manifest_path}")
    message(FATAL_ERROR "Cargo.toml not found at ${_manifest_path}")
  endif()

  string(TOLOWER "${SW_PROFILE}" _profile_lower)
  if(_profile_lower STREQUAL "release")
    set(_cargo_args build --release --manifest-path "${_manifest_path}")
    set(_profile_dir release)
  elseif(_profile_lower STREQUAL "debug")
    set(_cargo_args build --manifest-path "${_manifest_path}")
    set(_profile_dir debug)
  else()
    message(FATAL_ERROR "PROFILE must be release or debug")
  endif()

  string(TOLOWER "${SW_LINKAGE}" _linkage_lower)
  if(_linkage_lower STREQUAL "static")
    if(WIN32)
      set(_artifact_file "${SW_CARGO_TARGET_DIR}/${_profile_dir}/small_world.lib")
    else()
      set(_artifact_file "${SW_CARGO_TARGET_DIR}/${_profile_dir}/libsmall_world.a")
    endif()
    set(_artifact_kind STATIC)
  elseif(_linkage_lower STREQUAL "shared")
    if(WIN32)
      set(_artifact_file "${SW_CARGO_TARGET_DIR}/${_profile_dir}/small_world.dll")
      set(_import_lib "${SW_CARGO_TARGET_DIR}/${_profile_dir}/small_world.lib")
    elseif(APPLE)
      set(_artifact_file "${SW_CARGO_TARGET_DIR}/${_profile_dir}/libsmall_world.dylib")
    else()
      set(_artifact_file "${SW_CARGO_TARGET_DIR}/${_profile_dir}/libsmall_world.so")
    endif()
    set(_artifact_kind SHARED)
  else()
    message(FATAL_ERROR "LINKAGE must be STATIC or SHARED")
  endif()

  set(_stamp "${SW_CARGO_TARGET_DIR}/${SW_TARGET}_${_profile_dir}_${_linkage_lower}.stamp")
  set(_cargo_target "${SW_TARGET}_cargo_build")

  file(GLOB_RECURSE _rust_source_files CONFIGURE_DEPENDS
    "${SW_MANIFEST_DIR}/src/*.rs"
    "${SW_MANIFEST_DIR}/include/*.h"
  )
  set(_cargo_dep_files
    "${_manifest_path}"
    ${_rust_source_files}
  )
  if(EXISTS "${SW_MANIFEST_DIR}/Cargo.lock")
    list(APPEND _cargo_dep_files "${SW_MANIFEST_DIR}/Cargo.lock")
  endif()
  if(EXISTS "${SW_MANIFEST_DIR}/build.rs")
    list(APPEND _cargo_dep_files "${SW_MANIFEST_DIR}/build.rs")
  endif()

  add_custom_command(
    OUTPUT "${_stamp}"
    COMMAND "${CMAKE_COMMAND}" -E make_directory "${SW_CARGO_TARGET_DIR}"
    COMMAND "${CMAKE_COMMAND}" -E env "CARGO_TARGET_DIR=${SW_CARGO_TARGET_DIR}" "${SW_CARGO_EXECUTABLE}" ${_cargo_args}
    COMMAND "${CMAKE_COMMAND}" -E touch "${_stamp}"
    WORKING_DIRECTORY "${SW_MANIFEST_DIR}"
    DEPENDS ${_cargo_dep_files}
    BYPRODUCTS "${_artifact_file}"
    COMMENT "Building Rust small_world library (${_profile_lower}, ${_linkage_lower})"
    VERBATIM
  )

  add_custom_target("${_cargo_target}" DEPENDS "${_stamp}")

  add_library("${SW_TARGET}" ${_artifact_kind} IMPORTED GLOBAL)
  add_dependencies("${SW_TARGET}" "${_cargo_target}")
  set_target_properties("${SW_TARGET}" PROPERTIES
    IMPORTED_LOCATION "${_artifact_file}"
    INTERFACE_INCLUDE_DIRECTORIES "${SW_MANIFEST_DIR}/include"
  )

  if(WIN32 AND _linkage_lower STREQUAL "shared")
    set_target_properties("${SW_TARGET}" PROPERTIES IMPORTED_IMPLIB "${_import_lib}")
  endif()

  target_link_libraries("${SW_TARGET}" INTERFACE Threads::Threads)
  if(CMAKE_DL_LIBS)
    target_link_libraries("${SW_TARGET}" INTERFACE "${CMAKE_DL_LIBS}")
  endif()
  if(UNIX AND NOT APPLE)
    target_link_libraries("${SW_TARGET}" INTERFACE m)
  endif()

  if(NOT TARGET small_world::small_world)
    add_library(small_world::small_world ALIAS "${SW_TARGET}")
  endif()
endfunction()
