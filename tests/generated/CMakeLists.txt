# Generated strategies compiled and replayed, over headers atlc generated into ATLC_OUT (with the runtime in
# ATLC_OUT/atls); the `generated` test in tests/cpp.rs writes them and runs this project.
cmake_minimum_required(VERSION 3.20)
project(atlc_generated LANGUAGES CXX)

include(${CMAKE_CURRENT_SOURCE_DIR}/../support/atlc-test.cmake)
find_package(Catch2 3 CONFIG REQUIRED)
set(ATLC_OUT "" CACHE PATH "The generated headers and the runtime (atls/)")
if(NOT ATLC_OUT)
  message(FATAL_ERROR "set ATLC_OUT to the directory atlc wrote the headers and the runtime into")
endif()

enable_testing()
add_executable(test-generated test-one.cpp test-several.cpp test-identity.cpp test-edges.cpp test-pricing.cpp)
target_include_directories(test-generated PRIVATE ${ATLC_OUT} ${CMAKE_CURRENT_SOURCE_DIR}/../support)
target_link_libraries(test-generated PRIVATE Catch2::Catch2WithMain)
atlc_test_target(test-generated)
add_test(NAME test-generated COMMAND test-generated)
