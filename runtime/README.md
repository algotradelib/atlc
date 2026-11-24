# Runtime headers

`include/atls/` holds the C++ headers the generated strategies include:

| Header | Contents |
| --- | --- |
| `atls/ops.hpp` | The runtime's umbrella, which every generated header includes: it includes `expr.hpp`, `window.hpp`, `orders.hpp` and `data.hpp` |
| `atls/expr.hpp` | The undefined value, checked integer arithmetic and the three-valued operators on undefined values; `ATLS_NO_FMA` and the alias `atls::atl`, which the other headers share |
| `atls/window.hpp` | Rings and the built-in indicators in each arithmetic |
| `atls/orders.hpp` | The target rule and its pricing |
| `atls/data.hpp` | The record and the tape of `data {}` |
| `atls/universe.hpp` | One single-instrument strategy run over several instruments |
| `atls/measures.hpp` | The points of a finished replay's equity curve and the reductions of `measures {}`; it needs algotradelib built with Eigen |

They are plain hand-written files: `atlc` compiles them into its binary, so it carries the runtime it generates code for, and `atlc build --emit-runtime DIR` writes them into `DIR/atls/` as they are.
A project copies that whole directory: generated code includes `atls/ops.hpp`, and it includes its parts by their paths beside it.
A project that keeps generated strategies regenerates them and writes the runtime out with the same `atlc`: nothing checks a header against a runtime from another version.
They need algotradelib and a C++17 compiler, with floating-point contraction off; [the generated C++](../docs/reference/generated-code.md) says how to compile them.

## Tests

`tests/` holds the runtime's own tests (Catch2 3), against an installed algotradelib built with Eigen:

```sh
cmake -S runtime/tests -B build/runtime -DCMAKE_PREFIX_PATH=<algotradelib prefix>
cmake --build build/runtime
ctest --test-dir build/runtime --output-on-failure
```

`cargo test` runs them too, with the tests of generated strategies (`tests/generated/`) and the differential gate (`tests/gate/`), when `ATLC_TEST_PREFIX` names the algotradelib prefix.
`ATLC_TEST_SANITIZE=1` runs everything again with AddressSanitizer and UndefinedBehaviorSanitizer.
