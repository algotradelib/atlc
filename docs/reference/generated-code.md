# The generated C++

```sh
atlc build FILE.atls [-o OUT.hpp] [--indicators incremental|naive|exact] [--share on|off] [--history drop|keep]
atlc build --emit-runtime DIR
```

`atlc build` writes a program as one C++17 header, to `OUT.hpp` or to standard output; `--emit-runtime DIR` writes the runtime headers it includes into `DIR/atls/`.
Both may be given at once.
The exit code is 0 when everything was written, 1 when the program has errors (nothing is written, not even the runtime), and 2 on a usage or file error.

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

A program with `data {}` also states `using data_type = atls::DataRecord;` and `on_data`, the library's handler for external data (`atl::strategy::DataEvent<atls::DataRecord>`):
- `atls::DataRecord` holds `name_` (the value's index in `data {}`, the order of `atls_data()`), `instrument_` (optional) and `value_` (a `double`; NaN is an undefined value);
- `on_data` keeps the value with the record's `known_at_`; at each bar, before its series, every value is taken as known by the bar's `period_end_`, so a record received before a bar that ended earlier (a live backlog, warm-up history) is not read by that bar;
- one instrument: a record names no instrument or the program's; named instruments: no instrument (one value for the program); a universe: an instrument of the universe (that instrument's value) or none (every instrument's);
- any other record, or a `name_` outside `data {}`, throws `std::invalid_argument`, so a run stops rather than drop a value;
- `setup()` (and so `set`) forgets every value received.

A universe with `on_open` or `on_close` asks the library's `atl::strategy::SessionEdges` which slice opens its session and which closes it, by the runner's session window (`ctx.session()`).
On the opening slice it runs `on_open` before the slice's bars enter the series; on the closing slice, `on_close` after they do; a point at the open (a daily bar stamped at the open) enters no series.

Each handler call submits at most one `TargetPosition` per instrument: the last target it stated, in whole shares, with the struct's name in lower case as the tag.
A target stated again on a later bar is submitted again.

Targets are market orders unless the strategy is told otherwise:
- `set_order_pricing(type, offset)` prices every target from then on: `atl::OrderType::MARKET` (the default, with an offset of 0) or `atl::OrderType::LIMIT` with an `atl::Decimal64` offset at least 0 and below 0.3; it throws `std::invalid_argument` for another type, an offset outside that range, or an offset with a market order, and `set` and `setup()` keep it;
- with a limit, a target is priced from the price of the event that ran the handler, its side from the target against the net position, and a target that cannot be priced is not submitted ("Limit orders", below);
- `pricing_refusals()` counts the targets not submitted, by reason (`atls::ops::PricingRefusals`: `no_basis_price_`, `no_price_grid_`, `price_not_representable_`, `price_not_positive_`, `net_not_representable_`, and `total()`).

With market orders the pricing costs one check per submitted target: no portfolio scan, reference-data lookup or decimal arithmetic, and no change to the series or the warm-up.
With a limit, the strategy reads the net position once per target it prices (the one the handler read, if it reads `held`), and the instrument's reference data (`ctx.instrument(id)`), which a runner must supply.
A universe keeps each instrument's last event price, for its limits.

Parameters are run-time fields, so one compiled header runs every point of a tuning grid:
- `Params` has one field per parameter, `name_`, holding the program's default;
- `set(name, value)` sets one by name, and throws `std::invalid_argument` for an unknown name or an `int` that is not a whole number of magnitude at most 9e15;
- a constructor or `set` throws `std::invalid_argument` when a parameter is outside its bounds, naming the bound (`set` checks every bound again, so set a parameter before one whose bound depends on it).

An `int` result that does not fit in 64 bits throws `std::overflow_error` from the handler, constructor or `set` that computes it ([types and definedness](types-and-definedness.md), "Types"); the exception leaves the runner's loop, so the run stops.

For a runner, the struct also states `warmup_bars()`, the history requested by a live runner, including the newest decision bar ([program structure](program-structure.md), "Lookback and warm-up"); it may exceed the gate, the bars before the handlers run, which is internal to the struct.
It also states `atls_mode` (`"single"`, `"named"` or `"universe"`), `atls_session`, `atls_instruments()`, `atls_params()`, `atls_reads_data` (whether it has `data {}`) and `atls_data()` (its names, in order; empty without it).
`atls_on_open` and `atls_on_close` state whether the program has each; a program with either also states `session_opens()` and `session_closes()`, the sessions a bar has opened and closed so far, so a runner can refuse a run that met none, and `session_outside_bars()`, the bars wholly before a session's open or after its close (pre-market or after-hours data, which still enter the series).

### Limit orders

With `set_order_pricing(atl::OrderType::LIMIT, offset)`, the strategy gives each target it submits a limit price; the same code runs in a backtest and in a live run.
- The price is that of the event that ran the handler, taken exactly from the bar (its close): in `on_bar`, the decision bar's close; in `on_open`, the opening price (today's close is not visible there); in `on_close`, the close.
  Named instruments use each instrument's own bar; a universe keeps each instrument's last event price, so no instrument is priced from another's.
  The price does not depend on the program reading `close`.
- The side compares the target in whole shares (truncated toward zero) with the net position, summed over every holding row: above it, a buy; below it, a sell, for shorts and across zero alike (flat to -10 sells, -10 to -5 buys).
- The limit is `price × (1 + offset)` for a buy and `price × (1 − offset)` for a sell, computed exactly, then put on the instrument's tick grid on the safe side (a buy rounded down, a sell up).
- A target equal to the position is submitted without a limit, as with market orders, so that the runner still cancels unwanted working orders.
- An undefined target states nothing and does not erase an earlier valid one in the same handler; the price applies once, to the final target.
- A target that cannot be priced is not submitted, and is counted by reason in `pricing_refusals()`: the price missing or not positive (`no_basis_price_`), no reference data or an unknown price grid (`no_price_grid_`), arithmetic that does not fit a decimal (`price_not_representable_`; an offset of many digits, such as 0.299999999999999999, makes most products too large), a rounded price that is not positive (`price_not_positive_`), or a net position whose rows do not sum (`net_not_representable_`).
  A requested limit never becomes a market order.
  A program that reads `held` stops before stating any target when the net position does not sum, as it does with market orders, so that case is not counted here.

A runner that knows the day's price band hands the library the rounded limit, which the library clamps to the band when it is more aggressive than the band allows, keeps when it is passive, and refuses when its own rounding after a clamp moves it across the other bound.
The library sees only the rounded price: with a tick of 10, a band of 101 to 109 and a price of 105 at offset 0, a buy becomes 100 in the strategy and is kept as a passive order below the band.
A target's limit governs the orders the runner places to reach it; it does not reprice working orders on the same side that already suffice.
The same strategy computes the same requested price from the same inputs; a backtest and a live run can still differ in their fills, reference data, holdings and working orders.

How a limit fills, and when it expires, is up to the runner; [cmdatl's run settings](https://github.com/algotradelib/cmdatl/blob/main/docs/run-spec.md) describe its backtester.

## The program's facts

`atlc check FILE.atls --info` gives a tool what it needs to drive a program's header before building it: the struct's name, the mode, the instruments, whether it has session handlers, the parameters and the data values.
It prints exactly one JSON object on one line of standard output, whatever happened, and nothing on standard error:

```json
{"atlc_info":1,"status":"ok","atlc":"0.0.0-9000",
 "program":{"path":"/abs/pairs.atls","struct":"Pairs","mode":"named","instruments":["y","x"],"session":false,
            "params":[{"name":"lookback","type":"int"},{"name":"entry","type":"real"},{"name":"short","type":"bool"}],
            "data":[]},
 "diagnostics":[]}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `atlc_info` | integer | the format's version, 1 |
| `status` | `"ok"` or `"error"` | `"error"` exactly when a diagnostic is an error |
| `atlc` | string | the version of `atlc` |
| `program` | object or null | null on `"error"` |
| `program.path` | string | the program, absolute |
| `program.struct` | string | the generated struct's name, `atls::` omitted, as `atlc build` names it from the same path |
| `program.mode` | `"single"`, `"named"` or `"universe"` | the struct's `atls_mode` |
| `program.instruments` | array of strings | the names of `instruments {}`, in order; empty for the other modes |
| `program.session` | boolean | whether the program has `on_open` or `on_close` (the struct's `atls_session`) |
| `program.params` | array | each parameter's `name` and `type` (`"int"`, `"real"` or `"bool"`), in declaration order |
| `program.data` | array | each value of `data {}`, its `name` and `type` (`"real"`), in declaration order (a record's `name_`) |
| `diagnostics` | array | warnings and errors, below |

Each diagnostic is `{"severity", "code", "message", "file", "line", "origin"}`:
- `severity`: `"error"` or `"warning"`;
- `code`: a program diagnostic's code ([diagnostics](../diagnostics.md)), or one of `usage` (the command line), `unreadable` (a program that cannot be read), `program-name` (a program file named so that no struct can be, as `atlc build` refuses it);
- `file` and `line`: the program's absolute path (`.` components removed, `..` kept, links not resolved), as is `program.path`, and a line counted from 1; `line` is null for `unreadable` and `program-name`, and both are null for `usage`;
- `origin`: `"program"`, or `"command_line"` for `usage`.

| Exit code | Meaning |
| --- | --- |
| 0 | the program is valid (warnings allowed) |
| 1 | the program has errors |
| 2 | a usage error, a program that cannot be read, or a file name no struct can take |

A usage error is an unknown option, no file or two, `--info` given twice, or `--info` with `--json` or `--help`.
The file name is checked before the program, so a program named `1x.atls` gives only `program-name` with `--info`, though plain `atlc check` accepts it.
Exit code 101 is an internal error of `atlc`, the one case that writes to standard error.

## The runtime

| Header | Contents |
| --- | --- |
| `atls/ops.hpp` | Rings, the built-in indicators in each arithmetic, the three-valued operators on undefined values, the target rule and its pricing (`atls::ops::OrderPricing`) |
| `atls/universe.hpp` | One single-instrument strategy over several instruments: `atls::Universe<S>` forwards each bar to the strategy trading its instrument, `atls::UniverseBars<S>` each bar of a slice |

`atlc` carries the runtime headers it generates code for, compiled into it, and `--emit-runtime` writes them out as they are.
A generated header and a runtime from different `atlc` versions are not checked against each other: a project that keeps generated headers must regenerate them and write the runtime out (`atlc build --emit-runtime DIR`) with the same `atlc`.
The first line of a generated header names the `atlc` that wrote it.

The runtime includes algotradelib's `strategy` headers.
`atlc` runs no strategy: [cmdatl](https://github.com/algotradelib/cmdatl) backtests and runs generated strategies from the command line, and a project's own `main` compiles them as below ("Compiling exported strategies"), with cmdatl's installed `cmdatl/replay/` headers if it wants cmdatl's data loaders and report.
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
- Clang: nothing to set; the runtime and every generated function turn it off with a pragma;
- GCC: `-ffp-contract=off`;
- MSVC, Visual Studio 2022 or later: `/fp:precise`, never `/fp:contract` or `/fp:fast`.

No compiler may use `-ffast-math`, `-Ofast` or `/fp:fast`: an undefined value is a NaN, which those options assume never occurs.

## External C++

A program with external indicators or functions ([functions and indicators](functions-and-indicators.md), "External C++") names the user's C++ in namespace `atls::user`, and the project supplies it: atlc has no option for it.
After `namespace atls {` the header declares, in `namespace user`, each external indicator's class (`class NAME;`) and each external function's prototype (`double clip(double, double);`), then checks each class:

```cpp
static_assert(atls::ops::is_complete<user::ols>::value, "atls::user::ols is not defined: include a header that defines it before this one (with cmdatl, the run spec's header key)");
```

So a source that includes a generated header includes a header defining every external first:

```cpp
#include "user.hpp" // defines atls::user::ols and atls::user::clip
#include "generated/pairs.hpp"
```

- A class not defined there stops the compilation at that `static_assert` (other errors about the incomplete type may follow).
- Define each class with the key `class`, as the declaration does: one defined with `struct` compiles, but Clang warns (`-Wmismatched-tags`) that the mismatch may cause linker errors under the Microsoft C++ ABI.
- A function not defined fails at link time, naming `atls::user::NAME`.
- One defined with other parameter types declares an overload: the call then fails at link time, or at compile time when the two are ambiguous (`const double&` against `double`).
- One with another return type, or marked `noexcept` or `constexpr`, fails at compile time on the generated declaration.
- A function the user's header defines in that header is `inline` there; the generated declaration does not change that.
- The generated code calls a function through a static member that returns undefined first when a `real` or `bool` argument is, passing a `bool` as `true` or `false`; an indicator, through a struct holding the class in a `std::optional`, built on the first defined step after a reset.
- The pragma that turns contraction off covers the generated functions only, not the user's: the user's header turns it off in its own code, or the project's flags do (GCC's `-ffp-contract=off` and MSVC's `/fp:precise` apply to the whole source).
