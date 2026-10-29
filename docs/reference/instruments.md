# Instruments

A program sees instruments in one of three ways.

## One instrument (no block)

Bar fields are bare names (`close`); `target(shares)`.
Handlers: `on_start`, `on_bar`, `on_fill`, `on_stop` ([program structure](program-structure.md), "Handlers and when they run").

## Named instruments: `instruments { y; x; }`

- Names are the program's inputs, like Stan's `data {}`; the run spec's `[instruments]` maps each to a loaded symbol.
- Fields are read `y.close`, positions `y.held` (in `on_bar`); targets are `target(y, shares)`.
- Series are one value per period, so they may combine instruments (`real spread = y.close - hedge * x.close;`).
- A period missing any named instrument is skipped.
- `on_fill` is not available; session legs are not available.

## A universe: `universe {}`

- The same program runs for every instrument the run gives (or the ones `[instruments]` selects); series and state are per instrument.
- Cross-sectional operators combine, at one moment, every instrument that has that period's bar and is past its warm-up, over the values that are defined: `xs_mean(x)`, `xs_sum(x)`, `xs_count(x)`, `xs_rank(x)`.
  They are allowed only at the top level of a handler, never under `if` or `for`.
- Each top-level statement of a handler runs across every instrument before the next statement.
- `periods` counts every period so far; `bars` counts this instrument's periods (see below).
- Missing bars follow the program's missing-value policy ([missing values](missing-values.md)):
  - under `restart`, each instrument runs on its own bars, and `bars` counts them;
  - under `skip` and `poison`, every instrument takes every period: a period without its bar is a row of undefined values that advances the series and `bars` (so `bars == periods`), but does not run that instrument's handlers or count it in cross-sectional operators.
- `on_fill`, `on_start` and `on_stop` are not available.

## Session legs: `on_open` and `on_close`

- Universe programs only, instead of `on_bar`: either or both; the run feeds each day as an open and a close (`replay.fill = "legs"`, which requires daily bars).
- In `on_open`, only `open` is today's; reading today's `close`, `high` or `low` must not compile; series hold the previous close's values.
- `held` may be read in both.
- A target stated in `on_open` fills at the open, one in `on_close` at the close.

Not available yet: legs for one-instrument and named programs, and how legs map to the library's session phases in a live run.
