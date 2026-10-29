# Built-in functions and indicators

## Functions

Usable in series, in handlers and in function and indicator bodies; not in constant contexts (state initializers, bounds, window lengths).
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

Each window's arithmetic is the one the program's `build.indicators` names ([types](types-and-definedness.md), "Arithmetic").
Lookbacks: [program structure](program-structure.md); missing values: [missing values](missing-values.md).

## In universes

`xs_mean`, `xs_sum`, `xs_count`, `xs_rank`, `bars`, `periods`: see [instruments](instruments.md).
