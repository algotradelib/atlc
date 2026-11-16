# What every C++ test target of the project shares: C++17, the installed algotradelib, floating-point
# contraction off (the runtime and the generated headers turn it off with a pragma on Clang; GCC and MSVC
# need a flag), warnings, and AddressSanitizer with UndefinedBehaviorSanitizer when ATLC_SANITIZE is on.

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  set(CMAKE_BUILD_TYPE Release)
endif()

option(ATLC_SANITIZE "Build with AddressSanitizer and UndefinedBehaviorSanitizer" OFF)

find_package(algotradelib CONFIG REQUIRED)

# Whether this algotradelib declares backtest::MeasureInput, which `measures {}` reads (only with Eigen): the tests of
# measures are built only where it does, and otherwise say so with ATLC_PENDING_MEASURES, which tests/cpp.rs prints.
include(CheckCXXSourceCompiles)
set(CMAKE_REQUIRED_LIBRARIES algotradelib::algotradelib)
check_cxx_source_compiles(
  "#include <algotradelib/backtest>\nint main() { return sizeof(algotradelib::backtest::MeasureInput) > 0 ? 0 : 1; }"
  ATLC_HAS_MEASURE_INPUT)
unset(CMAKE_REQUIRED_LIBRARIES)
set(ATLC_PENDING_MEASURES "ATLC_PENDING: this algotradelib has no backtest::MeasureInput, so the tests of measures are not built")

# Adds the flags above to `target`.
function(atlc_test_target target)
  target_link_libraries(${target} PRIVATE algotradelib::algotradelib)
  target_compile_options(${target} PRIVATE
    $<$<CXX_COMPILER_ID:GNU>:-ffp-contract=off>
    $<$<CXX_COMPILER_ID:MSVC>:/fp:precise /W4>
    $<$<NOT:$<CXX_COMPILER_ID:MSVC>>:-Wall -Wextra>)
  if(ATLC_SANITIZE)
    if(MSVC)
      message(FATAL_ERROR "ATLC_SANITIZE: AddressSanitizer with UndefinedBehaviorSanitizer needs Clang or GCC")
    endif()
    target_compile_options(${target} PRIVATE -fsanitize=address,undefined -fno-sanitize-recover=all -fno-omit-frame-pointer)
    target_link_options(${target} PRIVATE -fsanitize=address,undefined)
  endif()
endfunction()
