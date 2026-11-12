# Program structure

A program is a set of blocks in any order.
Each block kind appears at most once, except `indicator`, which appears once per indicator.

## Blocks

- `params {}`: typed parameters with defaults and optional bounds.
  A default must be a literal (parentheses allowed).
  A bound may refer to earlier parameters (`real<upper=entry> exit`).
  A default outside its bounds must not compile; a value given at run time outside its bounds is refused when the strategy is built.
  Parameters are run-time values: changing one does not recompile.
- `series {}`: `real` or `bool` declarations (not `int`), recomputed every period, in dependency order.
  A series reads bar fields (`open high low close volume`), values of `data {}`, parameters and earlier series; it must not read `state` or `held`.
  Indicators are allowed only here: a streaming indicator must see every period, so it must not sit under an `if`.
  A series no handler reads, directly or through another series, is not computed, and the checker warns about it.
- `state {}`: typed values kept between periods, written only by handlers.
  A state is initialized from numbers and `int` or `real` parameters with `+ - * /` (no calls); a `bool` state from `true` or `false` only, not from a `bool` parameter.
- `options {}`: program-wide settings, each `key = value;` at most once:
  - `missing = restart | skip | poison`: the default missing-value policy ([missing values](missing-values.md));
  - `warmup = EXPR;`: at least this many periods before the periodic handlers' first call, which may come later for the lookbacks (below, "Lookback and warm-up").
- Handlers: see below.
- `instruments {}`, `universe {}`: see [instruments](instruments.md).
- `data {}`: values from outside the bars, `real NAME;` each (any data the strategy uses: a vendor's series, a file of events), given by the run (cmdatl's `[data.NAME]`):
  - a value is read like a bar field, in `series` and in the handlers a bar drives (`on_bar`, `on_open`, `on_close`, and `on_fill`, which reads the last bar's): the latest value known by the bar's end, undefined before the first, so indicators, lookbacks, warm-up and missing-value policies apply to it unchanged;
  - an indicator over a value counts bars, not records: under daily bars a monthly value repeats for a month, and `lag(x, 1)` is the previous bar's value;
  - in `on_open` it is the value known at the opening bar (unlike a bar field, whose series hold the bar before);
  - in a universe each instrument has its own value; with named instruments a value is the program's;
  - a value is never assigned, and `on_start` and `on_stop` cannot read it.

  ```atls
  data { real level; }
  series { real mean = sma(level, 5); }
  on_bar {
    if (level > mean) {
      target(1);
    } else {
      target(0);
    }
  }
  ```
- `functions {}`, `indicator`: see [functions and indicators](functions-and-indicators.md).
- There is no `replay {}` or `live {}` block: run settings belong to whatever runs the strategy (cmdatl's run settings, for one), live settings to the operator, not to a program.

## Handlers and when they run

| Handler | Programs | When |
| --- | --- | --- |
| `on_start` | one instrument, named instruments | once, before the first period, with no warm-up gate |
| `on_bar` | all; required unless the program has `on_open` or `on_close` | each period, after the series are updated, once the warm-up gate has passed |
| `on_open`, `on_close` | universes only, either or both, instead of `on_bar` | on the bar that opens each session and the bar that closes it, whatever the interval ([instruments](instruments.md)); at the open, series hold the values of the bar before |
| `on_fill` | one instrument only | after a fill; no series update precedes it |
| `on_stop` | one instrument, named instruments | once, after the last period |

A universe program with `on_start` or `on_stop` must not compile (the prototype accepted them but generated nothing for them).

## Statements in handlers

- Local declarations (fixed once declared), assignment to `state`, `if`/`else`, `for` over bounds built from literals and parameters, and `target`.
- `target(x)` states the target position in shares; with named instruments `target(y, x)`.
  A finite value of magnitude at most 1e12 is truncated toward zero; an undefined, infinite or larger value states nothing.
  The last valid target stated in a handler wins: `target(keep)` does not erase an earlier one.
  A target may be negative; whether a position may go below zero is a setting of the run (cmdatl's `replay.shorts`).
- `held` (the position, a `real`) is read in `on_bar`, and in a universe's `on_open` and `on_close`; with named instruments `y.held`, in `on_bar` only.
  `fill.price`, `fill.quantity` (`real`) and `fill.buy` (`bool`) are read in `on_fill`.
- There is no I/O, no `while`, no recursion and no order other than a target, unless the program calls external C++ ([functions and indicators](functions-and-indicators.md), "External C++"), which the compiler does not check.

## Names

A declaration must not use a keyword, a built-in's name, `lower` or `upper`.
This holds for every name a program declares: parameters, series, states, locals, functions and indicators, their arguments, loop variables, the names `enter` and `leave` give their values, and an indicator's window and states.
An indicator's values are the exception: they are read as `ind(...).name`, so they may take a built-in's name (`value real ema = ...;`).
Refusing built-in names for arguments, loop variables and an indicator's window, states and step names is intended behaviour (the prototype accepted them).

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
  - a user indicator: its inputs' largest lookback plus its window's length minus one, or its declared warm-up minus one, whichever is larger (an external indicator has no window: its warm-up's);
  - an expression of several inputs: the largest of their lookbacks;
  - under `missing = skip`: see [missing values](missing-values.md).
  The checker keeps each lookback as an expression of parameters, and refuses a program whose lookback expression is more than 256 levels high, the nesting limit of [syntax](syntax.md); a chain of indicators over indicators deepens it, by several levels for a `skip` window, a pair indicator or a user indicator.
  So `sma(sma(close, 20), 30)` needs 49 periods.
- The periodic handlers' first call comes after max(1, warmup, the lookbacks of every series any handler reads) periods: one gate for the program.
  `warmup` is set in `options { warmup = EXPR; }`, a whole number built from integer literals and integer parameters with `+ - *` and negation; unlike a window's length, it is not proved at least 1, and it only raises the gate, for a strategy that needs a longer start than its lookbacks.
  `warmup` is not a block: the former top-level statement `warmup EXPR;` is refused.
- After that gate the checker treats eventually-defined values as defined, but a handler can still see an undefined value: an undefined input restarts an indicator, and `skip`, `poison` or a domain error (`log(0)`) can leave a value undefined ([types and definedness](types-and-definedness.md)).
- A live runner requests more history than the gate where a window decides before it is whole (the generated struct's `warmup_bars()`, [generated code](generated-code.md)): enough for every finite window to be whole at its first decision.
  Each value needs its input's figure (1 for a bar field) plus its window's length minus one (the length for a lag and an RSI), the largest over several inputs, under every missing-value policy and for `ts_sum(x, n, m)` too.
  So `ts_sum(sign(close - close[k]), h - 1, 1)` is decided from bar k + 1 but needs k + h - 1 bars to be whole, and a live run's first decision then sees the windows a backtest sees on that date over the same bars.
  A user indicator's figure is its inputs' figure plus its window's length minus one or its declared warm-up minus one, whichever is larger; an external indicator's, its inputs' figure plus its warm-up minus one, so a warm-up shorter than the C++'s own window gives a live run less history than it needs.
  Under `missing = skip` a declared warm-up counts valid values, not bars, so where the input can be missing this figure is only a lower bound in bars, and a window over that indicator can still be partial at the first live decision.
  The checker keeps the figure as an expression of parameters, as a lookback, and refuses it past the same 256 levels.
  The gate is unchanged, so a backtest still decides from the gate over the windows there are.
  The figure covers finite windows only: an `ema`'s seed, `kf_*`, what an external indicator's C++ keeps beyond its declared warm-up, any state a user indicator keeps (in `update`, `enter` or `leave`, with a window or without: a declared window does not make `leave` undo `enter`), a user indicator's declared warm-up under `missing = skip` over an input that can be missing, a `poison` latch, any `state`, and the windows over them carry values from their own start, so a live run's first decision can differ from a backtest's on them.
  Raising `warmup` does not help a `state`, since states change only once the gate has passed.
  Nor does the figure cover a broker that returns fewer bars than requested, or floating-point sums whose result depends on where they started.
