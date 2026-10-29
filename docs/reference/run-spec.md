# Run spec

Planned: `atlc run SPEC [-P name=value] [--fills FILE]` reads a run spec, builds the strategy once per program shape, and prints the library's report as JSON.
Settings apply in this order, later winning: the program's `replay {}`, the spec, the command line.

The spec uses a subset of TOML: tables, dotted bare keys, double-quoted strings without escapes, numbers, booleans and single-line lists.
Unknown keys, duplicate keys and invalid values are errors.

```toml
strategy = "sma.atls"

[data]
store = "kis-krx/1d/005930"      # or source = "csv" with a column mapping
store_root = "/data/store"

[replay]
cash = 100000000
fill = "next-open"
shorts = false                   # also: from, to, market, commission, sell_tax, slippage_ticks

[params]
fast = 10

[build]
algotradelib = "/opt/algotradelib"   # an installed prefix
indicators = "incremental"
```

## `[data]`: what is loaded

- `store = "SOURCE/INTERVAL/SYMBOL"` with `store_root`: a Parquet store laid out as `<store_root>/bars/source=SOURCE/interval=INTERVAL/symbol=SYMBOL/bars.parquet`; the latest version of each bar is read, or the version known at `as_of`.
  Reading a store needs DuckDB; a build without it runs CSV data only.
- `source = "csv"`: OHLCV bars, one file per symbol, a symbol column, or a wide layout (one column per symbol); `columns.*` maps names; `time_format` is `unix`, `unix_ms`, `yyyymmdd` or `iso`; `timezone` is a fixed offset.
- `source = "wide"`: tables of one field each, a date column and one column per symbol, as published with Chan's book: `close` (required), and `open`, `high`, `low` when the program or fill needs them, with `names` for the symbols' file.
- A spec names data only: broker and vendor downloads fill the store in a separate step, so credentials never appear in a spec.

## `[instruments]`: names to symbols

Maps a program's named instruments to loaded symbols, or selects a universe's symbols; a universe takes every loaded symbol otherwise.

## `[replay]`

| `fill` | Data | Program |
| --- | --- | --- |
| `next-open` | any source | `on_bar` |
| `legs` | daily bars from `csv` or a store, or `source = "wide"` with `open`, `high`, `low` and `close` | a universe with `on_open`, `on_close` or both |
| `open-prev-close` | `source = "wide"` only (not a CSV wide layout) | `on_bar` |

`shorts` lets a position go below zero.

## `[build]`

- `algotradelib`: an installed prefix (or a configured build tree that exports the package); otherwise `CMAKE_PREFIX_PATH` or `algotradelib_DIR`, then CMake's own search.
  The generated strategy is linked to the `algotradelib::algotradelib` target, so its dependencies come from the library's package.
- `indicators`: `incremental`, `naive` or `exact` ([types](types-and-definedness.md), "Arithmetic").
- `duckdb`: the directory holding DuckDB's `duckdb.h` and shared library, for store runs; otherwise `DUCKDB_ROOT`, then the system's library paths.
- A build is reused while the generated code, the runtime headers, the build settings, the library installation and the compiler are unchanged; changing any of them builds again.

## Live

There is no live run yet.
The proposed design has three layers: the program (logic only), a run spec's `[live]` section (universe, interval, polling, risk limits, journal, control file), and the operator's configuration (broker, account, demo or real, hard ceilings), which a spec cannot change; the effective limit is the smaller of the spec's and the ceiling.

Still to define: every key's type and default, and the report's JSON schema.
