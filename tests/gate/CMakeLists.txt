# The differential gate, over headers atlc generated into ATLC_OUT (with the runtime in ATLC_OUT/atls); the
# `gate` test in tests/cpp.rs writes them and runs this project.
cmake_minimum_required(VERSION 3.20)
project(atlc_gate LANGUAGES CXX)

include(${CMAKE_CURRENT_SOURCE_DIR}/../support/atlc-test.cmake)
set(ATLC_OUT "" CACHE PATH "The generated headers and the runtime (atls/)")
if(NOT ATLC_OUT)
  message(FATAL_ERROR "set ATLC_OUT to the directory atlc wrote the headers and the runtime into")
endif()

enable_testing()
add_executable(gate gate.cpp)
target_include_directories(gate PRIVATE ${ATLC_OUT} ${CMAKE_CURRENT_SOURCE_DIR}/../support)
atlc_test_target(gate)
add_test(NAME gate COMMAND gate)
