# Program structure

A program is a set of blocks in any order.
Each block kind appears at most once, except `indicator`, which appears once per indicator.

## Blocks

- `params {}`: typed parameters with defaults and optional bounds.
  A default must be a literal (parentheses allowed).
  A bound may refer to earlier parameters (`int<lower=fast + 1> slow`).
  A default outside its bounds must not compile; a value given at run time outside its bounds is refused when the strategy is built.
  Parameters are run-time values: changing one does not recompile.
- `series {}`: `real` or `bool` declarations (not `int`), recomputed every period, in dependency order.
  A series reads bar fields (`open high low close volume`), parameters and earlier series; it must not read `state` or `held`.
  Indicators are allowed only here: a streaming indicator must see every period, so it must not sit under an `if`.
  A series no handler reads, directly or through another series, is not computed, and the checker warns about it.
- `state {}`: typed values kept between periods, initialized from literals and parameters (no calls), written only by handlers.
- `options {}`: program-wide settings; today only `missing` ([missing values](missing-values.md)).
- `warmup EXPR;`: a least number of periods before the periodic handler's first call.
- Handlers: see below.
- `instruments {}`, `universe {}`: see [instruments](instruments.md).
- `functions {}`, `indicator`: see [functions and indicators](functions-and-indicators.md).
- `replay {}`: run-spec settings carried in the program for sharing one file ([run spec](run-spec.md)); a run spec overrides them.
  There is no `live {}` block: live settings belong to the operator, not to a program.

## Handlers and when they run

| Handler | Programs | When |
| --- | --- | --- |
| `on_start` | one instrument, named instruments | once, before the first period, with no warm-up gate |
| `on_bar` | all; required unless the program uses legs | each period, after the series are updated, once the warm-up gate has passed |
| `on_open`, `on_close` | universes only, either or both, instead of `on_bar` | at each day's open and close; at the open, series hold the previous close's values |
| `on_fill` | one instrument only | after a fill; no series update precedes it |
| `on_stop` | one instrument, named instruments | once, after the last period |

A universe program with `on_start` or `on_stop` must not compile (the prototype accepted them but generated nothing for them).

## Statements in handlers

- Local declarations (fixed once declared), assignment to `state`, `if`/`else`, `for` over bounds built from literals and parameters, and `target`.
- `target(x)` states the target position in shares; with named instruments `target(y, x)`.
  A finite value of magnitude at most 1e12 is truncated toward zero; an undefined, infinite or larger value states nothing.
  The last valid target stated in a handler wins: `target(keep)` does not erase an earlier one.
  A target may be negative; whether the run lets a position go below zero is a run-spec setting.
- `held` (the position, a `real`) is read in `on_bar`, and in a universe's `on_open` and `on_close`; with named instruments `y.held`, in `on_bar` only.
  `fill.price`, `fill.quantity` (`real`) and `fill.buy` (`bool`) are read in `on_fill`.
- There is no I/O, no `while`, no recursion and no order other than a target.

## Names

A declaration must not use a keyword, a built-in's name, `lower` or `upper`.

## Lookback and warm-up

- Every window's length is built from integer literals and integer parameters with `+ - *`.
  The checker must prove from the parameters' bounds that every window is at least 1.
  It works with each value's range: with `int<lower=2> n` it proves `n * n - 3` at least 1, but with `int<lower=n + 1> m` it does not prove `m - n` at least 1, since the ranges of `m` and `n` are not related.
- A lag must be provably at least 1: `close[0]` and `lag(close, 0)` are refused (write `close`), and a negative lag reads a future bar.
- The lookback of a value, with L its input's lookback (a bar field has L = 1):
  - a full-window indicator of length n (`sma`, `stdev`, `ts_max`, ...): L + n - 1;
  - `lag(x, k)`, `x[k]`: L + k; `rsi(x, n)`: L + n;
  - `ts_sum(x, n, m)`: L + m - 1;
  - `ols_slope(y, x, n)`, `ols_intercept`: max(Ly, Lx) + n - 1; `kf_*`: max(Ly, Lx);
  - an expression of several inputs: the largest of their lookbacks;
  - under `missing = skip`: see [missing values](missing-values.md).
  So `sma(sma(close, 20), 30)` needs 49 periods.
- The periodic handlers' first call comes after max(1, warmup, the lookbacks of every series any handler reads) periods: one gate for the program.
- After that gate the checker treats eventually-defined values as defined, but a handler can still see an undefined value: an undefined input restarts an indicator, and `skip`, `poison` or a domain error (`log(0)`) can leave a value undefined ([types and definedness](types-and-definedness.md)).
