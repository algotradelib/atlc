# Run spec

```sh
atlc run SPEC.toml [-P name=value]... [--fills FILE]
atlc spec SPEC.toml [-P name=value]... [--json]
```

`atlc run` reads a run spec, checks it against its program, builds the strategy once per program shape, replays it in algotradelib's backtester, and prints the library's report as JSON on standard output.
`-P name=value` sets a parameter (a number, `true` or `false`); `--fills FILE` writes every fill as CSV.
Messages, including the build's progress, go to standard error.

| Exit code | Meaning |
| --- | --- |
| 0 | the report was printed |
| 1 | the program has errors (printed as `atlc check` prints them) |
| 2 | a usage, spec or file error; nothing was built |
| 3 | the build failed: no compiler or CMake, no algotradelib package, no DuckDB, or a compile error |
| 4 | the replay failed: unreadable data, a parameter outside its bounds, or an `int` overflow; its error is printed |

`atlc spec` checks a spec as `atlc run` would and prints its resolved settings, building nothing (below, "Checking a spec").

Settings apply in this order, later winning: the program's `replay {}`, the spec, the command line.
`replay {}` may set any key below but `strategy` and `params.*`; a parameter's default is in `params {}`.
A relative path is read against the directory of the file that sets it (the spec, or the program for `replay {}`), and `--fills` against the working directory.

```toml
strategy = "mean-reversion.atls"

[data]
source = "csv"
file = "bars.csv"
symbol = "005930"

[replay]
market = "kr"
cash = 100_000_000
commission = 0.00015
sell_tax = 0.002

[params]
lookback = 30

[build]
algotradelib = "/opt/algotradelib"
```

## The format

A spec is TOML, limited to a subset that every TOML reader reads the same way:
- `[table]` headers and `key = value` lines; keys are bare (letters, digits, `_`, `-`), dotted keys are allowed (`columns.open = "Open"`);
- strings in double quotes without escapes (a backslash is refused), or in single quotes, read as written (`'C:\data\bars.csv'`);
- decimal numbers (`100_000`, `0.00015`, `1.5e-4`), `true`, `false`, and arrays of strings or numbers on one line; a value that must be whole (an `int` parameter, `slippage_ticks`) is written as an integer, since TOML reads `2.0` as a float; cash and cost values are read exactly as decimals (`1.5e-4` is 0.00015): written out as a plain decimal, at most 18 digits without the leading zeros and at most 255 decimal places (`1e18` and `1e-256` are refused);
- `#` comments;
- spaces and tabs between the parts of a line, and no control character but tab, even in a string or a comment.

Errors, each with the file and line:
- an unknown key (including a misspelled one under `[build]` or `[orders]`), with the tables a spec has: `[data]`, `[instruments]`, `[replay]`, `[params]`, `[orders]`, `[build]`;
- a key set twice, a table defined twice, a name used both as a value and as a table, or a table made both by dotted keys and by a header (`data.file = ...` with `[data]`; TOML refuses it);
- a value of the wrong type or outside its range, or a key that does not apply to the run (a CSV column mapping for wide tables, say);
- a character TOML forbids: other whitespace outside a string (a no-break space, say), or a control character;
- what TOML has beyond the subset: escapes, quoted keys, inline tables, multi-line strings and arrays, arrays of tables, dates written bare (write `"2024-01-02"`).

A syntax error stops reading the file; past that, every error found is printed, not only the first.

## Keys

### The program

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `strategy` | path | required | the program |

### `[data]`: what is loaded

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `data.source` | `"wide"`, `"csv"` or `"store"` | `"store"` when `data.store` is set, else `"wide"` | where the bars come from |

`source = "wide"`: tables of one field each, a header and then rows of a yyyymmdd date and one cell per symbol (an empty cell, `NaN` or `nan` is missing), as Chan's book publishes them.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `data.file`, `data.close` | path | one is required | the closes (give one of the two) |
| `data.open`, `data.high`, `data.low` | path | none | the other fields, with the closes' dates and columns; `replay.fill = "legs"` needs all three |
| `data.names` | path | the tables' headers | a file whose second line names the columns (Chan's `*_stocks.csv`) |

A row is a bar with open, high and low equal to its close, unless the fill is `open-prev-close` or `legs`.

`source = "csv"`: OHLCV rows, split on commas without quoting.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `data.file` or `data.files` | path, or a list of paths | one is required | one file, or one file per symbol |
| `data.symbol` or `data.symbols` | string, or a list of strings | | the symbol of `data.file`, or of each of `data.files` |
| `data.symbol_column` | string | | instead: the column naming each row's symbol |
| `data.layout` | `"long"` or `"wide"` | `"long"` | `long`: rows of OHLCV; `wide`: a time column, then a column of closes per symbol (its header names the symbols) |
| `data.time` | string | `"ts"` | the time column |
| `data.time_format` | `"unix"`, `"unix_ms"`, `"yyyymmdd"` or `"iso"` | `"unix"` | seconds or milliseconds since 1970, a yyyymmdd date, or `YYYY-MM-DD` with an optional `HH:MM[:SS]` after `T` or a space and an optional `Z` or `+HH:MM`; a date that does not exist, a field out of range or anything after the time is refused |
| `data.timezone` | `"UTC"`, `"Z"` or `"+HH:MM"` | `"UTC"` | the fixed offset of `yyyymmdd` times and of `iso` times that name none (a time's own `Z` or `+HH:MM` wins); a named zone (`Asia/Seoul`) is not read |
| `data.stamp` | string | the `stamp` column if there is one, else `"close"` | `"open"` or `"close"` (bars stamped at their start or end), or a column holding one of them per row; a symbol's rows must share one stamp |
| `data.interval` | `"1m"`, `"5m"`, `"15m"`, `"30m"`, `"1h"`, `"1d"`, `"1w"` or `"1mo"` | `"1d"` | the bars' interval |
| `data.columns.open`, `data.columns.high`, `data.columns.low`, `data.columns.close`, `data.columns.volume` | string | `"open"`, `"high"`, `"low"`, `"close"`, `"volume"` | the long layout's columns |

A long file names its symbols one way: `data.symbol` with `data.file`, `data.symbols` with `data.files` (one per file), or `data.symbol_column`.
Every row has as many cells as the header.
A time must lie in the years 1 to 9999 and inside what the platform's clock holds, less two days: about the years 1678 to 2262 where it counts nanoseconds, as on Linux; a time outside is refused, never wrapped.
Rows outside `replay.from` and `replay.to` are dropped, a row with an empty close is skipped, a price that is not positive is refused with its line, and times must increase within a symbol.

`source = "store"`: a Parquet store, laid out as `<store_root>/bars/source=SOURCE/interval=INTERVAL/symbol=SYMBOL/bars.parquet` with the columns `ts`, `stamp`, `open`, `high`, `low`, `close`, `volume` and `fetched_at`.
The store keeps every fetched version of a bar; the replay reads, of each bar, the latest version.
A stamp is `open` or `close`, and a series' bars share one stamp.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `data.store` | `"SOURCE/INTERVAL/SYMBOL"`, or a list of them | required | the series read |
| `data.store_root` | path | required | the store's directory; there is no default, inside a checkout or out |
| `data.as_of` | instant: `"YYYY-MM-DD"`, optionally `THH:MM[:SS]` and `Z` or `+HH:MM` | the latest | read the latest version fetched at or before this instant |

Reading a store needs DuckDB (`[build]` below); a CSV or wide run does not.
A spec names data only: broker and vendor downloads fill the store in a separate step, so credentials never appear in a spec.

### `[instruments]`: names to symbols

| Key | Type | Meaning |
| --- | --- | --- |
| `instruments.NAME` | string | the symbol (a column or a symbol of the data) for a name |

- A program with `instruments {}` needs one entry per name and no other.
- A program with one instrument takes at most one entry, which picks its symbol; without one, the data must hold a single symbol.
- A universe takes the symbols of every entry (their names are free), or every symbol of the data without any.

### `[replay]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `replay.from`, `replay.to` | date: `"YYYY-MM-DD"`, or yyyymmdd as a number or string, from 1970 | the first and last bar | the dates replayed, inclusive, in UTC |
| `replay.market` | `"us"` or `"kr"` | `"kr"` when the first store coordinate's source starts with `kis`, `kiwoom` or `toss` or contains `krx`, else `"us"` | `us`: USD, any price; `kr`: KRW, prices on the KRX tick table |
| `replay.cash` | number above 0 | 100000 (`us`), 100000000 (`kr`) | the opening cash |
| `replay.commission` | number, 0 or more | 0 | the commission, a rate of each fill's value |
| `replay.min_commission` | number, 0 or more | 0 | the least commission of a fill |
| `replay.sell_tax` | number, 0 or more | 0 | the tax, a rate of each sell's value |
| `replay.slippage_ticks` | whole number, 0 to 1e9 | 0 | slippage, in ticks |
| `replay.fill` | `"next-open"`, `"open-prev-close"` or `"legs"` | `"next-open"` | when an order fills (below) |
| `replay.shorts` | boolean | `false` | whether a position may go below zero; no borrow is modelled |
| `replay.periods_per_year` | number above 0 | 252 | bars a year, for the annualized Sharpe ratio |

| `fill` | Data | Program | An order fills |
| --- | --- | --- | --- |
| `next-open` | any source | `on_bar` | at the next bar's open |
| `open-prev-close` | `source = "wide"` (not a CSV's wide layout) | `on_bar` | at the close it was decided on: each bar opens at the previous close |
| `legs` | daily bars: wide tables with opens, highs and lows, or a CSV or store | a universe with `on_open`, `on_close` or both | at the open for a target stated in `on_open`, at the close for one in `on_close` ([generated code](generated-code.md)) |

### `[params]`

| Key | Type | Meaning |
| --- | --- | --- |
| `params.NAME` | number, or `true`/`false` for a `bool` | a parameter's value (also `-P NAME=VALUE`) |

The name must be the program's; an `int` takes a whole number of magnitude at most 9e15, a `real` a number, a `bool` `true` or `false`.
Bounds are checked when the strategy is built, against the final values whatever order they were given in (a bound may name another parameter), and a value outside its bounds stops the replay (exit code 4).

### `[orders]`: how a target becomes an order

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `orders.type` | `"market"` or `"limit"` | `"market"` | a market order, or a limit priced from the price of the event that ran the handler |
| `orders.offset` | a plain decimal, at least 0 and below 0.3 | 0 | how far a limit lies past that price, as a fraction; only with `orders.type = "limit"` |

The offset is written as a plain decimal of at most 18 fractional digits (`0.002`, `0.299999999999999999`), which the replay reads exactly; an exponent (`2e-3`) is refused, never converted through a binary number.
The bound 0.3 is a chosen input bound, not an exchange's rule.
`replay {}` may set both keys; the layers are checked together, so an offset without `orders.type = "limit"` is refused whichever layer sets either.
Changing `[orders]` does not rebuild: the pricing is an argument, like the parameters.
How a limit is priced, and how it fills, is below ("Limit orders").

### `[build]`

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `build.algotradelib` | path | `CMAKE_PREFIX_PATH` or `algotradelib_DIR`, then CMake's own search | the prefix algotradelib was installed into (or a build tree that exports its package) |
| `build.duckdb` | path | `DUCKDB_ROOT`, then the system's paths | the directory of DuckDB's `duckdb.h` and shared library, for a store run |
| `build.indicators` | `"incremental"`, `"naive"` or `"exact"` | `"incremental"` | how windows are summed ([types](types-and-definedness.md), "Arithmetic") |

A `[build]` path holding `"`, `;` or `$` cannot be passed to CMake and is refused at its setting, before anything is built (`build.duckdb` only for a store run).

## The report

One JSON object, one field per line:

| Field | Type | Meaning |
| --- | --- | --- |
| `bars` | integer | bar events replayed (each instrument's bar counts once; a day read as legs counts three) |
| `executions` | integer | fills |
| `opening_equity`, `closing_equity` | decimal string | equity at the first and last recorded point |
| `total_return` | number or null | over the replay, as a fraction |
| `sharpe_per_period`, `sharpe_annualized` | number or null | the Sharpe ratio per bar, and annualized with `replay.periods_per_year` |
| `sortino_per_period` | number or null | the Sortino ratio per bar |
| `max_drawdown` | number or null | the worst peak-to-trough fall of equity, as a fraction |
| `turnover` | number or null | one-way turnover summed over the replay, as a fraction of equity |
| `commission_paid`, `slippage_paid`, `tax_paid` | decimal string or null | costs paid; null when the library has no total (nothing executed, no tax applies, the executions span currencies, or a sum did not fit) |
| `rejected` | integer | instructions not placed (reconciliation could not build them, or the replay cannot model them) |
| `refused` | integer | instructions a risk guard refused |
| `halted` | string or null | why the replay stopped early, if it did; read it before any figure |
| `aborted` | boolean | whether a halt cleared queued orders |
| `rejection_reasons` | object | each rejection and refusal reason, with its count (refusals as `guard:REASON`) |
| `pricing_refused` | integer | targets limit pricing did not submit ("Limit orders"); 0 with market orders |
| `pricing_refusal_reasons` | object | each pricing refusal reason with its count, when nonzero: `no_basis_price`, `no_price_grid`, `price_not_representable`, `price_not_positive`, `net_not_representable` |
| `caveats` | object | `gaps` (bars that did not follow their predecessor), `unfilled` (orders still queued at the end), `expired` (`DAY` orders withdrawn unfilled when their session ended, as they would expire live; on daily bars, after one bar), `unvalued_bars`, `multiple_instruments`, `cost_totals_incomplete`, `volume_ignored` (always true: fills are not capped by volume), `lowest_cash` (the lowest cash a fill left, as a decimal string, when below zero) and `shorts_borrow_nothing` (always true) |
| `first_bar`, `last_bar` | string | the first and last recorded times, ISO 8601 in UTC; absent when nothing was replayed |

A number is printed with ten significant digits; a figure the library leaves undefined (a return on zero equity, a ratio without variance) is null.

`--fills FILE` writes one line per fill, after the header `at,symbol,side,quantity,price,commission,tax`: the time (ISO 8601, UTC), the symbol, `buy` or `sell`, and decimals.

## Limit orders

With `orders.type = "limit"`, the generated strategy gives each target it submits a limit price; the same code runs in `atlc run`'s backtests and in any other driver of the strategy.
- The price is that of the event that ran the handler, taken exactly from the bar (its close): in `on_bar`, the decision bar's close; in `on_open`, the opening price (today's close is not visible there); in `on_close`, the close.
  Named instruments use each instrument's own bar; a universe keeps each instrument's last event price, so no instrument is priced from another's.
  The price does not depend on the program reading `close`.
- The side compares the target in whole shares (truncated toward zero) with the net position, summed over every holding row: above it, a buy; below it, a sell, for shorts and across zero alike (flat to -10 sells, -10 to -5 buys).
- The limit is `price × (1 + offset)` for a buy and `price × (1 − offset)` for a sell, computed exactly, then put on the instrument's tick grid on the safe side (a buy rounded down, a sell up).
- A target equal to the position is submitted without a limit, as before, so that the runner still cancels unwanted working orders.
- An undefined target states nothing and does not erase an earlier valid one in the same handler; the price applies once, to the final target.
- A target that cannot be priced is not submitted, and is counted by reason in the report's `pricing_refused` and `pricing_refusal_reasons`: the price missing or not positive (`no_basis_price`), no reference data or an unknown price grid (`no_price_grid`), arithmetic that does not fit a decimal (`price_not_representable`; an offset of many digits, such as 0.299999999999999999, makes most products too large), a rounded price that is not positive (`price_not_positive`), or a net position whose rows do not sum (`net_not_representable`).
  A requested limit never becomes a market order.
  A program that reads `held` stops before stating any target when the net position does not sum, as it does with market orders, so that case is not counted here.
- With market orders, the strategy does nothing more than check the setting once per target.

The backtester fills a limit when the next bar's range reaches it: a buy when the bar's low is at or below the limit, a sell when its high is at or above it.
The fill is credited at the limit, bounded into the bar (a buy at the lower of the limit and the bar's high), before and after slippage, so slippage never takes it through the limit; a better open is not credited.
A limit that does not fill is a `DAY` order: it is withdrawn when its session ends (on daily bars, after the next bar) and counted in `caveats.expired`; one still queued at the end is counted in `caveats.unfilled`.

The replay supplies no daily price limits, so a limit is never clamped in a backtest.
A run that knows the day's band hands the library the rounded limit, which the library clamps to the band when it is more aggressive than the band allows, keeps when it is passive, and refuses when its own rounding after a clamp moves it across the other bound.
The library sees only the rounded price: with a tick of 10, a band of 101 to 109 and a price of 105 at offset 0, a buy becomes 100 in the strategy and is kept as a passive order below the band.
A target's limit governs the orders the runner places to reach it; it does not reprice working orders on the same side that already suffice.
The same strategy computes the same requested price from the same inputs; a backtest and a live run can still differ in their fills, reference data, holdings and working orders.

## Checking a spec

`atlc spec SPEC.toml [-P name=value]... [--json]` reads and checks a spec exactly as `atlc run` prepares it: the spec and the program read, the program checked, its `replay {}` merged under the spec and `-P`, every setting checked, and the `[build]` paths checked as CMake would need them.
It builds nothing, touches no cache, opens no data, and needs no compiler, CMake or DuckDB.
Success means the configuration is valid; it does not mean the data exists, that CMake will find the library, or that the parameters are inside their bounds, which the strategy checks when it is built (`atlc run`'s exit code 4).

| Exit code | Meaning |
| --- | --- |
| 0 | the spec is valid (warnings allowed) |
| 1 | the program has errors |
| 2 | a usage, spec or file error |

Without `--json` it prints the resolved settings for a person, and the warnings and errors on standard error as `atlc run` prints them.
With `--json` it prints exactly one JSON object on one line of standard output, whatever happened, and nothing on standard error:

```json
{"atlc_spec":1,"status":"ok","atlc":"0.0.0-9000",
 "program":{"path":"/abs/mean-reversion.atls","struct":"MeanReversion"},
 "build":{"algotradelib":null,"duckdb":null,"indicators":"incremental"},
 "store":false,
 "orders":{"type":"limit","offset":"0.002"},
 "args":[["data.source","csv"],["data.file","/abs/bars.csv"],["data.symbol","AAPL"],["orders.type","limit"],["orders.offset","0.002"],["param.lookback","30"]],
 "warnings":[],"errors":[]}
```

| Field | Type | Meaning |
| --- | --- | --- |
| `atlc_spec` | integer | the format's version, 1 |
| `status` | `"ok"` or `"error"` | `"error"` exactly when `errors` is not empty |
| `atlc` | string | the version of `atlc` |
| `program` | object or null | the program's absolute `path`, and the `struct` its C++ is named (`atls::` omitted) |
| `build` | object or null | `algotradelib` and `duckdb` as the spec gives them, absolute, or null for the environment and CMake's search ("The build"; `atlc spec` discovers nothing), and `indicators` |
| `store` | boolean or null | whether the run reads a store, so needs DuckDB |
| `orders` | object or null | the effective `type` and `offset`, defaults included |
| `args` | array or null | the replay's arguments, in order, exactly as `atlc run` passes them |
| `warnings`, `errors` | arrays | diagnostics, below |

With `status` `"error"`, `program`, `build`, `store`, `orders` and `args` are null.
`args` holds what the layers set, not every effective value: defaults live in the replay driver and a parameter's in the generated strategy, so a consumer passes `args` on as given and does not fill in defaults.
Each pair `[key, value]` is one argument `key=value` to the built replay (`atls::run::main`), never two.
Values keep the text and encoding `atlc run` gives them: numbers as written, paths absolute, a setting's boolean as `true` or `false`, a parameter's boolean as `1` or `0`.
`atlc run`'s own `--fills FILE` adds `output.fills=FILE`, which is not part of the spec.

Each diagnostic is `{"severity", "code", "message", "file", "line", "origin"}`:
- `severity`: `"error"` or `"warning"`;
- `code`: a program diagnostic's code ([diagnostics](../diagnostics.md)), or one of `usage` (the command line: an unknown option, a missing or second spec, a malformed or repeated `-P`), `unreadable` (a spec or program that cannot be read), `program-name` (a program file named so that no C++ struct can be), `spec` (any other spec error);
- `file` and `line`: where it is, a line counted from 1; `line` is null for a file as a whole, and both are null when there is no one place (a setting that no layer gives, or the command line);
- `origin`: `"file"` (the spec, or the program's `replay {}`), `"command_line"` (an option or `-P`), or `"program"` (the program's own diagnostics).

## The build

`atlc run` writes a small CMake project for each program shape and builds it with the `cmake` on `PATH` (3.20 or later):
- the generated strategy, the runtime headers atlc carries ([generated code](generated-code.md)), and a `main.cpp` calling `atls::run::main`;
- linked to the `algotradelib::algotradelib` target of the package found by `build.algotradelib` alone when it is set (then also searched first for the package's dependencies), else by `CMAKE_PREFIX_PATH` or `algotradelib_DIR`, then CMake's own search;
- for a store run, DuckDB's prebuilt C library (`duckdb.h` and the shared library as DuckDB publishes it, tested with 1.5), from `build.duckdb` alone when it is set, else `DUCKDB_ROOT`, then the system's library paths; the executable finds the library by an rpath to its directory on macOS and Linux, and by a copy of `duckdb.dll` beside it on Windows, so it runs from anywhere;
- C++17 in Release, with floating-point contraction off (`-ffp-contract=off` on GCC, `/fp:precise` on MSVC, the runtime's pragmas on Clang);
- the compiler: `CXX` (a program's path or name, without arguments), else the first of `c++`, `g++` and `clang++` on `PATH`; on Windows, the one CMake's default generator picks.

The project lives in a cache entry: `ATLC_CACHE_DIR`, else `~/Library/Caches/atlc` on macOS, `%LOCALAPPDATA%\atlc\cache` on Windows, and `$XDG_CACHE_HOME/atlc` or `~/.cache/atlc` elsewhere; never inside a checkout unless `ATLC_CACHE_DIR` says so.
An entry is named by a key of everything the build reads:
- atlc's version, the generated header, `main.cpp`, the `CMakeLists.txt` (which holds `build.algotradelib`, `build.duckdb` and whether DuckDB is linked), and every runtime header compiled into atlc;
- the compiler's path, size, modification time and `--version` (on Windows: `CXX`, the `cl.exe` on `PATH` and the latest Visual Studio);
- the environment variables CMake reads: `CXXFLAGS`, `CPPFLAGS`, `LDFLAGS`, `CMAKE_GENERATOR`, `CMAKE_TOOLCHAIN_FILE`, `OPENSSL_ROOT_DIR`, `BOOST_ROOT`, `Boost_ROOT` and `Eigen3_ROOT`; `CMAKE_PREFIX_PATH`, `algotradelib_DIR`, `algotradelib_ROOT` and `ALGOTRADELIB_ROOT` unless `build.algotradelib` is set; `DUCKDB_ROOT` for a store run unless `build.duckdb` is set.

After a build the entry records what CMake found, and a later run reuses the executable only while that is unchanged: every file of the algotradelib package directory and of the library's headers, every CMake file the configure read (a toolchain file and the files it includes, the package files of algotradelib's dependencies, CMake's own modules, as CMake's file API lists them), the compiler, and DuckDB's library and header, by path, size and modification time.
A library reinstalled at the same prefix, a compiler upgraded in place, an edited toolchain file, or a dependency upgraded with new package files therefore builds again.
Parameters, data and replay settings are arguments of the executable, so changing them never builds.
The headers and libraries of algotradelib's dependencies (Boost, OpenSSL, Eigen) are not watched beyond their package files: after changing them in place, remove the cache directory.
Two runs building one entry at once take turns.

## Live

There is no live run yet.
The proposed design has three layers: the program (logic only), a run spec's `[live]` section (universe, interval, polling, risk limits, journal, control file), and the operator's configuration (broker, account, demo or real, hard ceilings), which a spec cannot change; the effective limit is the smaller of the spec's and the ceiling.
