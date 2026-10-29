# Runtime headers

Planned; empty for now.
`include/atls/` will hold the C++ headers the generated strategies call: windows and their missing-value policies, the built-in indicators, the target rule, the universe's lanes and the replay driver used by `atlc run`.
`atlc` will compile them into its binary, so the headers always match the code it generates, and `atlc build --emit-runtime DIR` will write them out for a project that includes generated strategies.
They will need algotradelib and a C++17 compiler, with their tests in `tests/` (CMake, Catch2).
