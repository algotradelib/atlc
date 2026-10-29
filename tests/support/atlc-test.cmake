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
