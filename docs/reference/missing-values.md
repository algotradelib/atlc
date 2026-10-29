# Missing values

A value is missing when its bar is absent or the value is NaN.
An infinity is not missing (it comes from arithmetic overflow; a division by zero is undefined instead), and `target` refuses it.
How a window treats a missing value is a property of the strategy, because it changes decisions.

```
options { missing = skip; }                                 # the program's default
series  { real ma = sma(close, 20, missing = restart); }    # one indicator's own policy
```

| Policy | A window with a missing value |
| --- | --- |
| `restart` (default) | starts again after it: the indicator is undefined until the window is full of values |
| `skip` | uses the values present: once n slots have passed, it is defined when at least `min` of them hold a value (`min = 1` by default; a smaller `min` never shortens the n slots) |
| `poison` | is undefined from then on, for the rest of the run; a series' own warm-up does not poison it |

## Per indicator

| Built-in | Policies | Under `skip` |
| --- | --- | --- |
| `sma`, `ts_sum` | `restart`, `skip`, `poison` | mean or sum of the values present |
| `stdev`, `stdev_pop` | `restart`, `skip`, `poison` | over the values present; `stdev` needs at least two |
| `zscore` | `restart`, `skip`, `poison` | needs a positive deviation and a defined newest value |
| `lag`, `x[k]` | `restart`, `skip`, `poison` | the value in that exact slot, which may be missing; `min =` is refused |
| `ema` | `restart`, `skip`, `poison` | a missing value is ignored and the previous output kept; defined once n values have been seen; `min =` is refused |
| `rsi`, `ts_max`, `ts_min` | `restart`, `poison` | not available |
| `ols_slope`, `ols_intercept`, `kf_*`, `ts_sum(x, n, m)` | always `restart` | not available; another program default does not change them, and a `missing =` argument is refused |

Under `skip` the lookback of `sma`, `ts_sum`, `stdev`, `stdev_pop` and `zscore` is max(n, L + min - 1), with L the input's lookback; `lag` keeps L + k.
For `ema` the lookback is a lower bound only: the value is defined once n values have been seen, however many periods that takes.
Refusing `min =` on `lag` and `ema` is intended behaviour (the prototype accepted and ignored it).

## In universes

Under `skip` and `poison`, a universe runs on a period clock: every instrument takes every period ([instruments](instruments.md)).
Under `restart`, each instrument runs on its own bars.

For comparison with other tools: `skip` is how Chan's MATLAB `smartMovingAvg` and `smartMovingStd` treat missing values; `restart` is pandas' `rolling` with its default minimum; `poison` is what a running sum that adds a NaN does.
