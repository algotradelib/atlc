# Runtime headers

`include/atls/` holds the C++ headers the generated strategies include:

| Header | Contents |
| --- | --- |
| `atls/ops.hpp` | Rings, the built-in indicators in each arithmetic, the three-valued operators on undefined values, the target rule |
| `atls/universe.hpp` | One single-instrument strategy run over several instruments |
| `atls/version.hpp` | The runtime's compatibility identifier, which every generated header checks |

`atlc` compiles them into its binary, so they always match the code it generates; `atlc build --emit-runtime DIR` writes them into `DIR/atls/` for a project that includes generated strategies.
They need algotradelib and a C++17 compiler, with floating-point contraction off; [the generated C++](../docs/reference/generated-code.md) says how to compile them.
The replay driver used by `atlc run` is planned.

After changing a header, run `BLESS=1 cargo test version_header` to write the new identifier into `atls/version.hpp`.

## Tests

`tests/` holds the runtime's own tests (Catch2 3), against an installed algotradelib built with Eigen:

```sh
cmake -S runtime/tests -B build/runtime -DCMAKE_PREFIX_PATH=<algotradelib prefix>
cmake --build build/runtime
ctest --test-dir build/runtime --output-on-failure
```

`cargo test` runs them too, with the tests of generated strategies (`tests/generated/`) and the differential gate (`tests/gate/`), when `ATLC_TEST_PREFIX` names the algotradelib prefix; `ATLC_TEST_SANITIZE=1` runs everything again with AddressSanitizer and UndefinedBehaviorSanitizer.
