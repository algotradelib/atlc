# The runtime headers' own tests, against an installed algotradelib:
#   cmake -S runtime/tests -B build/runtime -DCMAKE_PREFIX_PATH=<algotradelib prefix>
#   cmake --build build/runtime && ctest --test-dir build/runtime
cmake_minimum_required(VERSION 3.20)
project(atls_runtime_tests LANGUAGES CXX)

include(${CMAKE_CURRENT_SOURCE_DIR}/../../tests/support/atlc-test.cmake)
find_package(Catch2 3 CONFIG REQUIRED)

enable_testing()
add_executable(test-ops test-ops.cpp)
target_include_directories(test-ops PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../include)
target_link_libraries(test-ops PRIVATE Catch2::Catch2WithMain)
atlc_test_target(test-ops)
add_test(NAME test-ops COMMAND test-ops)
add_executable(test-args test-args.cpp)
target_include_directories(test-args PRIVATE ${CMAKE_CURRENT_SOURCE_DIR}/../include)
target_link_libraries(test-args PRIVATE Catch2::Catch2WithMain)
atlc_test_target(test-args)
add_test(NAME test-args COMMAND test-args)
