# The generated C++

```sh
atlc build FILE.atls [-o OUT.hpp] [--indicators incremental|naive|exact] [--share on|off] [--history drop|keep]
atlc build --emit-runtime DIR
```

`atlc build` writes a program as one C++17 header, to `OUT.hpp` or to standard output; `--emit-runtime DIR` writes the runtime headers it includes into `DIR/atls/`.
Both may be given at once.
The exit code is 0 when everything was written, 1 when the program has errors (nothing is written), and 2 on a usage or file error.

| Option | Values | Default |
| --- | --- | --- |
| `--indicators` | how windows are summed: `incremental`, `naive` or `exact` ([types and definedness](types-and-definedness.md), "Arithmetic") | `incremental` |
| `--share` | `on`: one ring per input series, shared by every indicator over it, and identical series expressions computed once; `off`: each indicator keeps its own ring and nothing is merged | `on` |
| `--history` | `drop`: the strategy declares `reads_bar_history = false`, since its series keep their own lookback, so the runner keeps no bar history for it; `keep`: no declaration | `drop` |

`--share` and `--history` change memory and speed, not results.

## The strategy

The header holds one struct in namespace `atls`, named after the file: its stem in CamelCase (`mean-reversion.atls` gives `atls::MeanReversion`).
A stem that does not start with a letter, or that gives `Params` or `Lane`, is refused.
The struct passes the library's `ATL_ASSERT_STRATEGY` and runs in its backtester and live runners.

| Program | Constructors | Handlers |
| --- | --- | --- |
| One instrument | `S(InstrumentId)`, `S(InstrumentId, Params)`, and `S(std::vector<InstrumentId>)` taking the first | `on_bar`, for that instrument's bars only; `on_fill` for its fills |
| Named instruments | `S(std::vector<InstrumentId>)`, in the order of `instruments {}`; another count throws | `on_bars`, one slice per period; a period missing any named instrument is skipped |
| Universe | `S(std::vector<InstrumentId>)`, every instrument traded | `on_bars`; with `on_open` or `on_close`, see below |

`on_start` and `on_stop` become handlers of the same names.

A universe with `on_open` or `on_close` reads each day as three one-minute bars stamped 0, 1 and 2 minutes after midnight: the open alone, the day's bar, and the close alone (the run spec's `fill = "legs"`).
`on_open` runs on the first and `on_close` on the second, so a target stated at the open fills at the day's open and one stated at the close fills at the close.

Each handler call submits at most one `TargetPosition` per instrument: the last target it stated, in whole shares, with the struct's name in lower case as the tag.
A target stated again on a later bar is submitted again.

Parameters are run-time fields, so one compiled header runs every point of a tuning grid:
- `Params` has one field per parameter, `name_`, holding the program's default;
- `set(name, value)` sets one by name, and throws `std::invalid_argument` for an unknown name or an `int` that is not a whole number of magnitude at most 9e15;
- a constructor or `set` throws `std::invalid_argument` when a parameter is outside its bounds, naming the bound (`set` checks every bound again, so set a parameter before one whose bound depends on it).

An `int` result that does not fit in 64 bits throws `std::overflow_error` from the handler, constructor or `set` that computes it ([types and definedness](types-and-definedness.md), "Types"); the exception leaves the runner's loop, so the run stops.

For a runner, the struct also states `warmup_bars()` (the bars before the handlers run), `atls_mode` (`"single"`, `"named"` or `"universe"`), `atls_legs`, `atls_instruments()` and `atls_params()`.

## The runtime

| Header | Contents |
| --- | --- |
| `atls/ops.hpp` | Rings, the built-in indicators in each arithmetic, the three-valued operators on undefined values, the target rule |
| `atls/universe.hpp` | One single-instrument strategy over several instruments: `atls::Universe<S>` forwards each bar to the strategy trading its instrument, `atls::UniverseBars<S>` each bar of a slice |
| `atls/version.hpp` | `ATLS_RUNTIME_ID`, the runtime's compatibility identifier |

The runtime is compiled into `atlc`, so the copy `--emit-runtime` writes always matches the headers the same `atlc` generates.
`ATLS_RUNTIME_ID` is computed from the other runtime headers; every generated header checks it and stops with an `#error` naming the `atlc` version when the runtime it finds has another identifier.

The runtime includes algotradelib's `strategy` headers.
`--indicators naive` solves `ols_slope` and `ols_intercept` with Eigen, so it needs an algotradelib built with Eigen (`ATL_USE_EIGEN=ON`, the default); without it, such a strategy fails to compile with a message saying so.

## Compiling exported strategies

A project that includes generated headers compiles them as C++17 and links algotradelib's CMake target:

```cmake
find_package(algotradelib CONFIG REQUIRED)
add_executable(backtest main.cpp)
target_compile_features(backtest PRIVATE cxx_std_17)
target_include_directories(backtest PRIVATE generated)   # the headers and atls/
target_link_libraries(backtest PRIVATE algotradelib::algotradelib)
target_compile_options(backtest PRIVATE
  $<$<CXX_COMPILER_ID:GNU>:-ffp-contract=off>
  $<$<CXX_COMPILER_ID:MSVC>:/fp:precise>)
```

Floating-point contraction must stay off ([types and definedness](types-and-definedness.md), "Arithmetic"):
- Clang: nothing to set; the runtime and the generated code turn it off with a pragma;
- GCC: `-ffp-contract=off`;
- MSVC, Visual Studio 2022 or later: `/fp:precise`, never `/fp:contract` or `/fp:fast`.

No compiler may use `-ffast-math`, `-Ofast` or `/fp:fast`: an undefined value is a NaN, which those options assume never occurs.
