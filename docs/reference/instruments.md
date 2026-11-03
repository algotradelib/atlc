# Instruments

A program sees instruments in one of three ways.

## One instrument (no block)

Bar fields are bare names (`close`); `target(shares)`.
Handlers: `on_start`, `on_bar`, `on_fill`, `on_stop` ([program structure](program-structure.md), "Handlers and when they run").

## Named instruments: `instruments { y; x; }`

- Names are the program's inputs, like Stan's `data {}`; the run maps each to a loaded symbol (cmdatl's `instruments.NAME`).
- Fields are read `y.close`, positions `y.held` (in `on_bar`); targets are `target(y, shares)`.
- Series are one value per period, so they may combine instruments (`real spread = y.close - hedge * x.close;`).
- A period missing any named instrument is skipped.
- `on_fill`, `on_open` and `on_close` are not available.

## A universe: `universe {}`

- The same program runs for every instrument the run gives (or the ones `[instruments]` selects); series and state are per instrument.
- Cross-sectional operators combine, at one moment, every instrument that has that period's bar and is past its warm-up, over the values that are defined: `xs_mean(x)`, `xs_sum(x)`, `xs_count(x)`, `xs_rank(x)`.
  They are allowed only at the top level of a handler, never under `if` or `for`.
  A statement's reductions are computed, over every active instrument, before the statement runs on any of them, whatever the conditions around them (`c ? xs_sum(x) : keep`, `c and xs_mean(x) > 0`): a condition cannot guard what a reduction evaluates, so an `int` overflow in its argument stops the run even where no instrument takes that arm.
- Each top-level statement of a handler runs across every instrument before the next statement.
- `periods` counts every period so far (a point at the open is none); `bars` counts this instrument's periods (see below).
- Missing bars follow the program's missing-value policy ([missing values](missing-values.md)):
  - under `restart`, each instrument runs on its own bars, and `bars` counts them;
  - under `skip` and `poison`, every instrument takes every period: a period without its bar is a row of undefined values that advances the series and `bars` (so `bars == periods`), but does not run that instrument's handlers or count it in cross-sectional operators.
- `on_fill`, `on_start` and `on_stop` are not available.

## Session handlers: `on_open` and `on_close`

- Universe programs only, instead of `on_bar`: either or both.
- They run on whatever bars the run feeds, by the run's session window (the library's `SessionEdges`): `on_open` on the bar that opens each session, `on_close` on the bar that closes it.
  - On minute or hourly bars, the session's first bar (known at its end, a period after the open) and the bar whose period holds the close (on KRX minute data, the 15:30 closing print).
  - On daily bars split at the open (cmdatl's `data.open_point`), the point at the open and the day's bar at the close.
  - On plain daily bars a session is one bar, which runs both: `on_open`'s targets are replaced by `on_close`'s, so cmdatl asks for `data.open_point` or intraday bars.
  - A session with no bar reaching its close runs no `on_close`.
- In `on_open`, only the opening bar's `open` is new: reading its `close`, `high` or `low` must not compile, and series hold the values of the bar before it; the bar enters the series after `on_open`.
- A point at the open (one price, a daily bar stamped at the open) enters no series; every other bar does, whether or not a handler runs on it.
- `held` may be read in both.
- When a target fills is the run's fill rule: at the next bar's open by default, or at the price the handler decided on (cmdatl's `replay.fill = "close"`).

Not available yet: session handlers for one-instrument and named programs, and with `on_bar` in one program.
