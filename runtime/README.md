# Runtime headers

`include/atls/` holds the C++ headers the generated strategies and `atlc run` include:

| Header | Contents |
| --- | --- |
| `atls/ops.hpp` | Rings, the built-in indicators in each arithmetic, the three-valued operators on undefined values, the target rule |
| `atls/universe.hpp` | One single-instrument strategy run over several instruments |
| `atls/run.hpp` | The replay driver `atlc run` builds each strategy with: settings, bar readers (wide tables, CSV, a Parquet store through DuckDB's C API), the backtester, the report as JSON |

They are plain hand-written files: `atlc` compiles them into its binary, so it carries the runtime it generates code for, and `atlc build --emit-runtime DIR` writes them into `DIR/atls/` as they are.
A project that keeps generated strategies regenerates them and writes the runtime out with the same `atlc`: nothing checks a header against a runtime from another version.
They need algotradelib and a C++17 compiler, with floating-point contraction off; [the generated C++](../docs/reference/generated-code.md) says how to compile them.
`atls/run.hpp` reads a store only when compiled with `ATLS_DUCKDB` and linked to DuckDB's C library; `atlc run` does both when a spec reads one.

## Tests

`tests/` holds the runtime's own tests (Catch2 3), against an installed algotradelib built with Eigen:

```sh
cmake -S runtime/tests -B build/runtime -DCMAKE_PREFIX_PATH=<algotradelib prefix>
cmake --build build/runtime
ctest --test-dir build/runtime --output-on-failure
```

`cargo test` runs them too, with the tests of generated strategies (`tests/generated/`), the differential gate (`tests/gate/`) and the runs of `atlc run` (`tests/run.rs`, which test `atls/run.hpp`), when `ATLC_TEST_PREFIX` names the algotradelib prefix; the store runs also need `ATLC_TEST_DUCKDB`, the directory of DuckDB's C library.
`ATLC_TEST_SANITIZE=1` runs everything again with AddressSanitizer and UndefinedBehaviorSanitizer (of the runs, the CSV one).
