# Types and definedness

## Types

- `int`: a signed integer, held as a 64-bit integer in the generated C++.
  Literals, defaults and parameter values pass through a double on the way in, so a magnitude above 9e15 is refused: as a literal or default by the checker, as a run-time parameter value by the strategy's setter.
  An `int` literal is a number written without `.` or exponent, wherever it appears (a `real` default too: write `1e16`); 9e15 itself is accepted; the settings of `replay {}` are run-spec values, not literals in this sense.
  Refusing large literals and defaults is intended behaviour (the prototype accepted them and lost precision past 2^53).
- `int` arithmetic (`+`, `-`, `*`, unary `-`, `abs`) is exact in 64 bits, wherever it appears (handlers, series, functions, indicators, constant expressions).
  An `int` is always defined, so a result that does not fit in 64 bits cannot be undefined: it stops the strategy with an error (in the generated C++, `std::overflow_error` naming the operation and its operands), raised where the operation runs, before any target that bar.
  This is intended behaviour: a run stops rather than trade on a wrong number.
- `real`: an IEEE 754 double.
- `bool`: `true` or `false`.
- `int` widens to `real` where a `real` is expected.
  Nothing converts a `real` to an `int`: `round` and `floor` return an integer-valued `real`, so `int n = round(x);` must not compile.

## Undefined values

A `real` may be undefined (NaN): a window not yet full, a division by zero, a missing value, a missing `open`, `log(0)`.

- Arithmetic with an undefined operand is undefined.
- A comparison with an undefined side is undefined.
- `and` and `or` are three-valued: a defined `false` decides `and`, a defined `true` decides `or`; otherwise an undefined side makes the result undefined.
- An `if` whose condition is undefined takes neither branch.
- `keep` is an undefined `real`; `target(keep)` states nothing.
- `defined(x)` tests for NaN; an infinity is defined (but `target` refuses it).

## What is evaluated

Since an `int` overflow stops the run, what an expression evaluates is part of its meaning:
- In handlers, functions and indicators, `c ? a : b` evaluates `c`, then only the arm `c` selects, and neither arm when `c` is undefined.
- There, `a and b` and `a or b` evaluate `a` first, and `b` only when `a` does not decide the result (a defined `false` for `and`, a defined `true` for `or`).
- A user function call evaluates every argument; then an undefined `real` or `bool` argument makes it undefined without running the body.
- A loop's bounds are evaluated once, before its first pass; a range ending at the largest 64-bit integer stops after it.
- In `series {}`, every part of every series expression is computed on every bar, both arms of `?:` and both sides of `and` and `or` included, since an indicator anywhere in a series must see every bar (a function called there still follows the rules above inside its body).
- In a universe handler, a cross-sectional reduction (`xs_mean`, `xs_sum`, `xs_count`, `xs_rank`) is computed over every active instrument before its statement runs, whatever the conditions around it: a condition cannot guard what a reduction evaluates ([instruments](instruments.md)).

## Defined `int` and `bool` state

`int` and `bool` values in `state {}` are always defined.
Assigning a value that may be undefined to one must not compile.
`defined(x)` narrows `x` to defined where it is true: `defined(x) ? x > 1 : false`, and inside `if (defined(x)) { ... }`.

## What the checker knows

Every series value is one of:
- always defined: bar fields other than `open`, literals, arithmetic without `/`;
- eventually defined: an indicator over such a value, treated as defined once its lookback has passed;
- maybe undefined: `open`, a division whose divisor is not a provably nonzero literal or parameter, anything under `missing = skip` or `poison` in a universe.

Only maybe-undefined values carry run-time checks.
"Treated as defined" is the checker's assumption, not a promise that the value is never NaN ([program structure](program-structure.md), "Lookback and warm-up").

## Arithmetic

How a window is summed decides trades at exact ties: on a day when an open equals its 20-day average to the cent, a running sum and the same window added again can differ in the last bit, and `open > average` comes out differently.
atls names the arithmetic of each indicator, and a build can choose among three (`atlc build --indicators`, or the run spec's `build.indicators`):

| Indicator | `incremental` (default) | `naive` | `exact` |
| --- | --- | --- | --- |
| `sma`, `ts_sum(x, n)` | running sum: add the newest value, subtract the oldest | the window summed again, oldest first | sliding sum without subtraction ("two stacks") |
| `rsi` | running sums of gains and losses | summed again | sliding sums without subtraction |
| `stdev`, `stdev_pop`, `zscore` | running sums of values and squares | two passes over the window (mean, then squared deviations) | running sums, as `incremental` |
| `ts_max`, `ts_min` | a monotonic queue | the window scanned again | a monotonic queue |
| `ols_slope`, `ols_intercept` | running sums | least squares solved with Eigen | running sums |
| `sma`, `ts_sum`, `stdev`, `stdev_pop`, `zscore` under `missing = skip` | running sums over the values present | summed again; deviations in two passes over the values present | running sums, as `incremental` |
| `ts_sum(x, n, m)` | running sum, in every mode | | |
| `ema`, `kf_*`, `lag` | their recurrence or ring, in every mode | | |
| user indicators | the arithmetic their author wrote, in every mode | | |

`exact` selects subtraction-free sums for `sma`, `ts_sum(x, n)` and `rsi`, which leave no cancellation residue; the other indicators keep the algorithm the table names, and none of this is decimal arithmetic.
A result computed another way (by another tool, or in decimal) may differ at an exact tie.

Floating-point contraction (fused multiply-add) is off in generated code and in the runtime: with Clang by pragma, with GCC by `-ffp-contract=off`, and with MSVC by `/fp:precise`, which leaves contraction off from Visual Studio 2022 (17.0) on; earlier MSVC versions are not supported.
`atlc run` sets these flags; a project that compiles exported strategies itself must set them too, and must not add `/fp:contract` ([the generated C++](generated-code.md), "Compiling exported strategies").
Contraction is not the only difference between compilers and math libraries (`log`, `sqrt` and evaluation order may differ in the last bit); atls promises identical results only for one compiler and platform.
