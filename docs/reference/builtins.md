# Built-in functions and indicators

## Functions

Usable in series, in handlers, in function and indicator bodies and in measures; not in constant contexts (state initializers, bounds, window lengths).
Each is undefined when an argument is, except `defined`, which is always defined.

| Function | Result |
| --- | --- |
| `abs(x)`, `max(a, b)`, `min(a, b)` | as usual |
| `sign(x)` | -1, 0 or 1 |
| `log(x)` | natural log; undefined at or below 0 |
| `sqrt(x)` | undefined below 0 |
| `round(x)` | the nearest integer as a `real`, halves away from zero |
| `floor(x)` | the integer below or equal as a `real` |
| `defined(x)` | whether `x` is not NaN; narrows `x` ([types](types-and-definedness.md)) |

## Indicators (`series {}` only)

| Indicator | Definition |
| --- | --- |
| `lag(x, k)`, `x[k]` | `x` k periods ago; k at least 1 |
| `sma(x, n)` | mean of the last n values |
| `ema(x, n)` | seeded with the mean of the first n values, then alpha 2/(n+1) |
| `rsi(x, n)` | Cutler's RSI: simple means of the gains and losses over the last n moves (n+1 values); 50 when nothing moved |
| `ts_max(x, n)`, `ts_min(x, n)` | largest and smallest of the last n values |
| `ts_sum(x, n)`, `ts_sum(x, n, m)` | sum of the last n values; with m, defined once m of them are in |
| `stdev(x, n)` | sample standard deviation (divisor n - 1) |
| `stdev_pop(x, n)` | population standard deviation (divisor n), as Chan's `smartstd` |
| `zscore(x, n)` | `(x - sma(x, n)) / stdev(x, n)`; undefined when the deviation is 0 |
| `ols_slope(y, x, n)`, `ols_intercept(y, x, n)` | least squares of y on x with an intercept over the last n pairs |
| `kf_beta(y, x, delta, ve)`, `kf_error`, `kf_sd` | Chan's Kalman-filter hedge ratio, its forecast error and the error's deviation; one filter shared by the three |

Each window's arithmetic is the one the build's `--indicators` names ([types](types-and-definedness.md), "Arithmetic").
Lookbacks: [program structure](program-structure.md); missing values: [missing values](missing-values.md).

## Reductions (`measures {}` only)

Each reduces its one argument over every point of the replay's equity curve, oldest first, using the points where the argument is defined.

| Reduction | Over the points where its argument is defined | With none |
| --- | --- | --- |
| `cum_sum(x)` | the sum, oldest first | 0 |
| `cum_mean(x)` | the mean | undefined |
| `cum_stdev(x)` | the sample standard deviation (divisor n - 1), in two passes: the mean, then the squared deviations from it | undefined below two |
| `cum_min(x)`, `cum_max(x)` | the smallest, the largest | undefined |
| `cum_first(x)`, `cum_last(x)` | the first, the last defined value | undefined |
| `cum_count(c)` | how many points have `c` defined and true | 0 |

- `x` is a number (an `int` widens) and `c` a `bool`; the result is a `real`, `cum_count`'s a whole number.
- A reduction takes one argument and no named one (`missing =` is refused): it always skips undefined values, and `options { missing }` does not touch it ([missing values](missing-values.md)).
- A reduction inside another's argument is refused: name the inner one as an earlier measure (`real m = cum_mean(ret); real v = cum_mean((ret - m) * (ret - m));`).
- The points are the curve as the replay recorded it: warm-up periods included (flat equity, zero returns), one per bar or per period as the run records them, up to the halt of a halted replay, none when the run records no curve.
- `cum_max(drawdown)` is the maximum drawdown and `cum_mean(ret) / cum_stdev(ret)` the per-period Sharpe ratio, as the library computes them (to rounding: the library may order its additions differently).
- No quantile and no financial figure (`cagr`, `calmar`) is built in: write them from these, or in your own C++ (`extern real f(real x);` called in a measure).

## In universes

`xs_mean`, `xs_sum`, `xs_count`, `xs_rank`, `bars`, `periods`: see [instruments](instruments.md).
