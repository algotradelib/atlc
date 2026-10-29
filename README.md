# atlc

The compiler for atls, a short strategy language for [algotradelib](https://github.com/algotradelib/algotradelib).
You write a strategy in a few checked blocks; `atlc` turns it into a C++ strategy for the library's runner.
In the prototype, compiled strategies ran as fast in a backtest as the same strategies written by hand in C++.

```atls
# sma.atls: long while the fast average is above the slow one.
params {
  int<lower=1> fast = 20;
  int<lower=fast + 1> slow = 60;
  int<lower=0> quantity = 100;
}
series {
  real fast_avg = sma(close, fast);
  real slow_avg = sma(close, slow);
}
on_bar {
  if (fast_avg > slow_avg) {
    target(quantity);
  } else {
    target(0);
  }
}
```

Status: in development, before its first release.
Only `atlc --version` works today; the commands below are planned, and the language may change without notice.

```sh
atlc check sma.atls                 # parse and check, no C++ toolchain needed
atlc build sma.atls -o sma.hpp      # the C++ strategy, for your own algotradelib project
atlc run sma.toml -P fast=10        # replay it from a run spec and print the report as JSON
```

## Install

```sh
cargo install --git https://github.com/algotradelib/atlc
```

`atlc check` and `atlc build` will need nothing else.
`atlc run` compiles the generated strategy, so it will also need a C++17 compiler, CMake and an installed algotradelib; see [its install guide](https://github.com/algotradelib/algotradelib#install).
Reading bars from a Parquet store will also need DuckDB.
If algotradelib is installed under a prefix CMake does not search, set `CMAKE_PREFIX_PATH` to that prefix, or name it in the run spec.

## The language

A program is a set of blocks:

| Block | What it holds |
| --- | --- |
| `params {}` | Typed parameters with bounds (`int<lower=1> fast = 20;`); a tuning grid runs them all from one build |
| `series {}` | Values computed every bar before the handlers, including indicators (`sma`, `ema`, `stdev`, `ols_slope`, ...) |
| `state {}` | Values the handlers keep from bar to bar |
| `on_start`, `on_bar`, `on_fill`, `on_stop` | Handlers; the only output is a target position |
| `instruments {}`, `universe {}` | Several named instruments, or the same program over many |
| `options {}` | How missing values enter windows: `restart`, `skip` or `poison` |
| `functions {}`, `indicator` | Your own functions and streaming indicators |

A program has no I/O and no unbounded loop, and cannot read a future bar: the checker refuses it.
Data, dates, costs and fills come from a run spec, so one program can be replayed on other data or tuned without editing it; a `replay {}` block may carry defaults for sharing one file.
Live trading is not part of atls yet.
The full definition is the [language reference](docs/reference/README.md).

## Acknowledgements

This project was developed with the assistance of generative AI tools.

## Disclaimer

atlc is released under the MIT license; see [LICENSE](LICENSE).
The software is provided as is, without warranty of any kind, and the authors are not liable for any loss, including trading losses, arising from its use.
Nothing in this repository is investment advice.
