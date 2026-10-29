# The store test's generator: a small Parquet store written by DuckDB itself (`make-store STORE_ROOT`).
cmake_minimum_required(VERSION 3.20)
project(atlc_make_store LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_path(DUCKDB_INCLUDE duckdb.h PATHS "${DUCKDB_DIR}" PATH_SUFFIXES include NO_DEFAULT_PATH)
find_library(DUCKDB_LIBRARY duckdb PATHS "${DUCKDB_DIR}" PATH_SUFFIXES lib NO_DEFAULT_PATH)
if(NOT DUCKDB_INCLUDE OR NOT DUCKDB_LIBRARY)
  message(FATAL_ERROR "no DuckDB in DUCKDB_DIR=${DUCKDB_DIR}")
endif()

add_executable(make-store make-store.cpp)
target_include_directories(make-store PRIVATE "${DUCKDB_INCLUDE}")
target_link_libraries(make-store PRIVATE "${DUCKDB_LIBRARY}")
get_filename_component(duckdb_dir "${DUCKDB_LIBRARY}" DIRECTORY)
set_target_properties(make-store PROPERTIES BUILD_RPATH "${duckdb_dir}"
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin" RUNTIME_OUTPUT_DIRECTORY_RELEASE "${CMAKE_BINARY_DIR}/bin")
if(WIN32)
  add_custom_command(TARGET make-store POST_BUILD COMMAND "${CMAKE_COMMAND}" -E copy_if_different "${duckdb_dir}/duckdb.dll" "$<TARGET_FILE_DIR:make-store>")
endif()
