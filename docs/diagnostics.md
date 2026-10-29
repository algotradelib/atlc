# Diagnostics

Every diagnostic `atlc check` prints has a stable code, in brackets after `error` or `warning` and as `code` in the `--json` form.
A code keeps its meaning; a message's wording may change.
Each entry below gives the code, what it means, and an example from the tests (`tests/ui/`).

## Output

`atlc check FILE [--json]` exits with 0 when the program is valid (warnings allowed), 1 when it has errors, and 2 on a usage error or a file it cannot read.
Exit code 101 is an internal error of `atlc` (a bug, or a thread it cannot start): `atlc: internal error` on stderr, after any panic message; please report it.
By default, diagnostics go to stderr in the style of rustc.
With `--json`, each diagnostic is one JSON object on its own line on stdout:

```json
{"severity":"error","code":"unknown-name","message":"unknown name `clse`","file":"a.atls","span":{"start":{"line":3,"column":10},"end":{"line":3,"column":14},"byte_start":42,"byte_end":46},"label":"not declared","secondary":[],"notes":["did you mean `close`?"]}
```

- `severity`: `error` (the program is refused) or `warning`.
- `span`: lines and columns count from 1, columns in characters; `end` is the position just past the span; `byte_start` and `byte_end` are byte offsets.
- `label`: what the caret under the span says; `secondary`: other spans with their labels (`first declared here`); `notes`: the lines after `=`.
- Warnings are printed only for a valid program.

## Tokens

### `invalid-number`

A number the lexer cannot read.
Not reachable today: every token the lexer takes for a number is a valid one.

### `unclosed-string`

A string (in `replay {}`) without its closing `"` on the same line.

```
error[unclosed-string]: this string is never closed
 --> tests/ui/syntax/unclosed-string.atls:2:12
  |
2 |   market = "krx;
  |            ^^^^^ add `"`
```

From `tests/ui/syntax/unclosed-string.atls`.

### `unexpected-character`

A character that is not part of the language.

```
error[unexpected-character]: unexpected character `@`
 --> tests/ui/syntax/unexpected-character.atls:2:12
  |
2 |   target(1 @ 2);
  |            ^ not part of the language
```

From `tests/ui/syntax/unexpected-character.atls`.

## Syntax

### `expected-token`

A punctuation mark or word the grammar requires here is missing, such as `;`, `}` or `in`.

```
error[expected-token]: expected `)` to close `sma(`, found `;`
 --> tests/ui/syntax/missing-paren.atls:2:24
  |
2 |   real a = sma(close, 5;
  |                        ^ expected `)`
```

From `tests/ui/syntax/missing-paren.atls`.

### `expected-name`

A name was expected (an instrument, a parameter, an argument...).

```
error[expected-name]: expected an instrument name, found a number
 --> tests/ui/syntax/expected-name.atls:2:3
  |
2 |   1;
  |   ^ expected an instrument name
```

From `tests/ui/syntax/expected-name.atls`.

### `keyword-as-name`

A keyword used as a name; keywords are reserved everywhere.

```
error[keyword-as-name]: `if` is a keyword and cannot be used as a parameter name
 --> tests/ui/syntax/keyword-as-name.atls:2:7
  |
2 |   int if = 1;
  |       ^^ reserved
```

From `tests/ui/syntax/keyword-as-name.atls`.

### `expected-block`

Something other than a block at the top level of the program.

```
error[expected-block]: expected a block, found `;`
 --> tests/ui/syntax/stray-token.atls:4:1
  |
4 | ;
  | ^ a program is made of blocks
  = blocks: instruments {}, universe {}, options {}, functions {}, indicator NAME(...) {}, params {}, series {}, state {}, warmup EXPR;, on_start {}, on_bar {}, on_open {}, on_close {}, on_fill {}, on_stop {}, replay {}
```

From `tests/ui/syntax/stray-token.atls`.

### `unknown-block`

A word at the top level that names no block.

```
error[unknown-block]: unknown block `strategy`
 --> tests/ui/syntax/unknown-block.atls:1:1
  |
1 | strategy {
  | ^^^^^^^^ not a block
  = blocks: instruments {}, universe {}, options {}, functions {}, indicator NAME(...) {}, params {}, series {}, state {}, warmup EXPR;, on_start {}, on_bar {}, on_open {}, on_close {}, on_fill {}, on_stop {}, replay {}
```

From `tests/ui/syntax/unknown-block.atls`.

### `v0-syntax`

The prototype's first syntax (`param`, `let`, `target = ...`).

```
error[v0-syntax]: `param` is v0 syntax
 --> tests/ui/syntax/v0-syntax.atls:1:1
  |
1 | param fast = 20
  | ^^^^^ not a block
  = v0 `param`, `let` and `target =` became `params {}`, `series {}` and `on_bar { target(...); }`
```

From `tests/ui/syntax/v0-syntax.atls`.

### `live-block`

A `live {}` block: live settings belong to the run spec and the operator, not to a program.

```
error[live-block]: a program has no `live {}` block
 --> tests/ui/syntax/live-block.atls:3:1
  |
3 | live { broker = "kis"; }
  | ^^^^ not a block
  = live settings (broker, account, limits) belong to the run spec and the operator's configuration, never to the strategy
```

From `tests/ui/syntax/live-block.atls`.

### `expected-setting`

A `replay {}` setting does not start with a (dotted) key.

```
error[expected-setting]: expected a setting name, found a number
 --> tests/ui/syntax/expected-setting.atls:2:3
  |
2 |   1 = 2;
  |   ^ e.g. `cash = 100000;`
```

From `tests/ui/syntax/expected-setting.atls`.

### `expected-literal`

A `replay {}` setting's value is not a number, a string or a bool.

```
error[expected-literal]: expected a number, a string or a bool, found `close`
 --> tests/ui/syntax/expected-literal.atls:2:10
  |
2 |   cash = close;
  |          ^^^^^ a setting's value is a literal
```

From `tests/ui/syntax/expected-literal.atls`.

### `expected-indicator-part`

An indicator holds something other than `window`, `state`, `update`, `enter`, `leave`, `value` or `warmup`.

```
error[expected-indicator-part]: expected a part of an indicator, found `return`
 --> tests/ui/syntax/expected-indicator-part.atls:3:3
  |
3 |   return 1;
  |   ^^^^^^ not a part
  = parts: window w[n];  state real s = 0;  update { }  enter(v) { }  leave(v) { }  value real v = ...;  warmup n;
```

From `tests/ui/syntax/expected-indicator-part.atls`.

### `expected-type`

A declaration does not start with `int`, `real` or `bool`.

```
error[expected-type]: expected a type (`int`, `real` or `bool`), found `n`
 --> tests/ui/syntax/expected-type.atls:2:3
  |
2 |   n = 1;
  |   ^ a declaration starts with its type
```

From `tests/ui/syntax/expected-type.atls`.

### `unknown-type`

A type name from another language (`double`, `float`).

```
error[unknown-type]: unknown type `double`
 --> tests/ui/syntax/unknown-type.atls:2:3
  |
2 |   double n = 1;
  |   ^^^^^^ write `real`
```

From `tests/ui/syntax/unknown-type.atls`.

### `expected-bound`

A parameter bound other than `lower=` or `upper=`.

```
error[expected-bound]: expected `lower` or `upper`, found `min`
 --> tests/ui/syntax/expected-bound.atls:2:7
  |
2 |   int<min=1> n = 2;
  |       ^^^ a bound is `lower=EXPR` or `upper=EXPR`
```

From `tests/ui/syntax/expected-bound.atls`.

### `expected-expression`

An expression was expected.

```
error[expected-expression]: expected an expression, found `;`
 --> tests/ui/syntax/two-errors.atls:6:21
  |
6 |   real b = (close + ;
  |                     ^ expected an expression
```

From `tests/ui/syntax/two-errors.atls`.

### `nesting-too-deep`

A program nested more than 256 levels deep (an implementation limit): blocks, `else if`, parentheses and every level of an expression's tree count, one per operator of a chain; or a lookback built from parameters that deep.

```
error[nesting-too-deep]: the program nests deeper than 256 levels here
 --> tests/ui/syntax/nesting-too-deep.atls:2:265
  |
2 |   target(((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((((1)))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))))));
  |                                                                                                                                                                                                                                                                         ^ too deep
  = blocks, `else if`, parentheses and each operator of a chain count one level each; split the expression with locals or series
```

From `tests/ui/syntax/nesting-too-deep.atls`.

### `c-style-operator`

`&&`, `||` or `!` instead of `and`, `or`, `not`.

```
error[c-style-operator]: use `and`
 --> tests/ui/syntax/c-and.atls:5:17
  |
5 |   if (a > close && held == 0) {
  |                 ^^ write `and`
```

From `tests/ui/syntax/c-and.atls`.

### `assign-in-expression`

`=` inside an expression, where `==` compares.

```
error[assign-in-expression]: `=` assigns; compare with `==`
 --> tests/ui/syntax/assign-in-condition.atls:2:12
  |
2 |   if (held = 0) {
  |            ^ write `==`
```

From `tests/ui/syntax/assign-in-condition.atls`.

### `chained-comparison`

Two comparisons in a row (`a < b < c`); comparisons do not chain.

```
error[chained-comparison]: comparisons do not chain
 --> tests/ui/syntax/chained-comparison.atls:6:17
  |
6 |   if (a < close < b) {
  |                 ^ a second comparison
  |       --------- this is already a comparison
  = write `a < b and b < c`
```

From `tests/ui/syntax/chained-comparison.atls`.

## Names and blocks

### `built-in-name`

A declaration takes the name of a built-in (a bar field, a function, an indicator, `lower`, `upper`...).

```
error[built-in-name]: `close` is a built-in name
 --> tests/ui/check/built-in-name.atls:2:7
  |
2 |   int close = 1;
  |       ^^^^^ choose another name
```

From `tests/ui/check/built-in-name.atls`.

### `duplicate-name`

Two declarations with one name.

```
error[duplicate-name]: `n` is declared twice
 --> tests/ui/check/duplicate-name.atls:5:7
  |
2 |   int n = 1;
  |       - first declared here
5 |   int n = 0;
  |       ^ second declaration
```

From `tests/ui/check/duplicate-name.atls`.

### `shadowed-name`

A local or an indicator state takes a name already in use.

```
error[shadowed-name]: `x` is already a name
 --> tests/ui/check/shadowed-name.atls:4:10
  |
4 |     real x = 2;
  |          ^ choose another name
```

From `tests/ui/check/shadowed-name.atls`.

### `unknown-name`

A name that is not declared; a near name is suggested.

```
error[unknown-name]: unknown name `fats`
 --> tests/ui/check/unknown-name.atls:5:23
  |
5 |   real a = sma(close, fats);
  |                       ^^^^ not declared
  = did you mean `fast`?
```

From `tests/ui/check/unknown-name.atls`.

### `duplicate-block`

A block kind written twice (only `indicator` may repeat).

```
error[duplicate-block]: a second `on_bar`
 --> tests/ui/check/duplicate-block.atls:2:1
  |
1 | on_bar { target(1); }
  | ------ first here
2 | on_bar { target(0); }
  | ^^^^^^ second
```

From `tests/ui/check/duplicate-block.atls`.

### `missing-on-bar`

No `on_bar` (and no session legs): nothing would ever state a target.

```
error[missing-on-bar]: the program has no `on_bar`
 --> tests/ui/check/missing-on-bar.atls:1:1
  |
1 | params {
  | ^ nothing ever states a target
  = add `on_bar { target(...); }`
```

From `tests/ui/check/missing-on-bar.atls`.

### `unused-series`

Warning: a series no handler reads, directly or through another series; it is not computed.

```
warning[unused-series]: series `unused` is never read
 --> tests/ui/check/unused-series.atls:2:8
  |
2 |   real unused = sma(close, 5);
  |        ^^^^^^ not computed
```

From `tests/ui/check/unused-series.atls`.

## Literals and types

### `int-literal-too-large`

An `int` literal above 9e15 in magnitude: it would lose precision on its way through a double.

```
error[int-literal-too-large]: this `int` literal is above 9e15 in magnitude
 --> tests/ui/check/int-literal-too-large.atls:4:14
  |
4 |   int over = 9000000000000001;
  |              ^^^^^^^^^^^^^^^^ too large
  = an `int` passes through a double on its way into the strategy, and loses precision past 2^53
  = write a `real` with an exponent (`1e16`) if a real is meant
```

From `tests/ui/check/int-literal-too-large.atls`.

### `type-mismatch`

A value of the wrong type for a declaration, an assignment or a return.

```
error[type-mismatch]: type mismatch: `up` is `bool` but the value is `real`
 --> tests/ui/check/type-error.atls:2:13
  |
2 |   bool up = close - close[1];
  |             ^^^^^^^^^^^^^^^^ this is `real`
```

From `tests/ui/check/type-error.atls`.

### `operand-type`

An operator applied to the wrong type (`-` or arithmetic on a bool, `not`, `and`, `or` on a number).

```
error[operand-type]: type mismatch: `not` needs a `bool`
 --> tests/ui/check/operand-type.atls:2:12
  |
2 |   bool b = not close;
  |            ^^^^^^^^^ operand is `real`
  = compare first, e.g. `not (x > 0)`
```

From `tests/ui/check/operand-type.atls`.

### `condition-not-bool`

A condition (`if`, `?:`) that is not a `bool`.

```
error[condition-not-bool]: type mismatch: a condition must be `bool`
 --> tests/ui/check/type-error.atls:5:7
  |
5 |   if (close) {
  |       ^^^^^ this is `real`
  = compare it, e.g. `x != 0`
```

From `tests/ui/check/type-error.atls`.

### `ternary-arms`

The two arms of `?:` have incompatible types.

```
error[ternary-arms]: type mismatch: the two arms of `?:` differ
 --> tests/ui/check/ternary-arms.atls:2:26
  |
2 |   target(close > 1 ? 1 : true);
  |                          ^^^^ this is `bool`
  |                      - this is `int`
```

From `tests/ui/check/ternary-arms.atls`.

### `argument-type`

A function's, operator's or indicator's argument has the wrong type.

```
error[argument-type]: `abs` needs a number
 --> tests/ui/check/argument-type.atls:2:14
  |
2 |   target(abs(close > 1));
  |              ^^^^^^^^^ this is `bool`
```

From `tests/ui/check/argument-type.atls`.

### `int-series`

A series declared `int`; series are `real` or `bool`.

```
error[int-series]: a series is `real` or `bool`
 --> tests/ui/check/int-series.atls:2:3
  |
2 |   int n = 1;
  |   ^^^ write `real`
  = bar values are reals; whole-number series are not supported
```

From `tests/ui/check/int-series.atls`.

## Parameters and constants

### `default-not-literal`

A parameter's default is not a literal (parentheses allowed).

```
error[default-not-literal]: a parameter's default is a number
 --> tests/ui/check/default-not-literal.atls:2:11
  |
2 |   int n = 1 + 1;
  |           ^^^^^ not a literal
  = defaults are literals; bounds may refer to earlier params
```

From `tests/ui/check/default-not-literal.atls`.

### `int-default-not-whole`

An `int` parameter's default is not a whole-number literal.

```
error[int-default-not-whole]: `n` is an `int` but its default is not a whole number
 --> tests/ui/check/int-default-not-whole.atls:2:11
  |
2 |   int n = 2.5;
  |           ^^^ write a whole number, or declare it `real`
```

From `tests/ui/check/int-default-not-whole.atls`.

### `bool-default-not-bool`

A `bool` parameter's default is not `true` or `false`.

```
error[bool-default-not-bool]: `b` is a `bool`; its default is `true` or `false`
 --> tests/ui/check/bool-default-not-bool.atls:2:12
  |
2 |   bool b = 1;
  |            ^ not a bool
```

From `tests/ui/check/bool-default-not-bool.atls`.

### `numeric-default-is-bool`

An `int` or `real` parameter's default is a bool.

```
error[numeric-default-is-bool]: `r` is `real` but its default is a bool
 --> tests/ui/check/numeric-default-is-bool.atls:2:12
  |
2 |   real r = true;
  |            ^^^^ not a number
```

From `tests/ui/check/numeric-default-is-bool.atls`.

### `default-below-bound`

A parameter's default is below its lower bound.

```
error[default-below-bound]: the default of `slow` (10) is below its lower bound
 --> tests/ui/check/bound-violated.atls:3:30
  |
3 |   int<lower=fast + 1> slow = 10;
  |                              ^^ must be at least 21
  |             -------- `fast + 1` is 21 with the defaults
```

From `tests/ui/check/bound-violated.atls`.

### `default-above-bound`

A parameter's default is above its upper bound.

```
error[default-above-bound]: the default of `n` (20) is above its upper bound
 --> tests/ui/check/default-above-bound.atls:2:21
  |
2 |   int<upper=10> n = 20;
  |                     ^^ must be at most 10
  |             -- `10` is 10 with the defaults
```

From `tests/ui/check/default-above-bound.atls`.

### `bool-bounds`

Bounds on a `bool` parameter.

```
error[bool-bounds]: a `bool` has no bounds
 --> tests/ui/check/bool-bounds.atls:2:3
  |
2 |   bool<lower=0> b = true;
  |   ^^^^ remove the bounds
```

From `tests/ui/check/bool-bounds.atls`.

### `not-constant`

A value that must be fixed before the run (a window, a bound, a state's initial value, a loop bound) reads something that changes during it.

```
error[not-constant]: a loop bound must be fixed before the run
 --> tests/ui/check/loop-bound-not-fixed.atls:5:15
  |
5 |   for (i in 1:held) {
  |               ^^^^ `held` changes every bar
  = use numbers and params only
```

From `tests/ui/check/loop-bound-not-fixed.atls`.

### `not-whole-number`

A count of bars (a window, the warm-up, a loop bound) that is not a whole number.

```
error[not-whole-number]: `sma`'s window is a whole number of bars
 --> tests/ui/check/not-whole-number.atls:2:23
  |
2 |   real a = sma(close, 2.5);
  |                       ^^^ not a whole number
```

From `tests/ui/check/not-whole-number.atls`.

### `const-param-not-int`

A constant built from a parameter of the wrong type: a count of bars from a parameter that is not an `int`, or a bound or a state's initial value from a `bool` parameter.

```
error[const-param-not-int]: `sma`'s window is built from `int` params
 --> tests/ui/check/const-param-not-int.atls:5:23
  |
5 |   real a = sma(close, n);
  |                       ^ `n` is `real`
```

From `tests/ui/check/const-param-not-int.atls`.

### `count-division`

A count of bars that divides; `/` gives a `real`.

```
error[count-division]: `sma`'s window cannot divide
 --> tests/ui/check/count-division.atls:5:23
  |
5 |   real a = sma(close, n / 2);
  |                       ^^^^^ `/` gives a real
  = a count of bars is a whole number
```

From `tests/ui/check/count-division.atls`.

### `warmup-not-int`

An indicator's `warmup` that is not a whole number.

```
error[warmup-not-int]: a warm-up is a whole number of steps
 --> tests/ui/check/warmup-not-int.atls:2:10
  |
2 |   warmup 1.5;
  |          ^^^ this is `real`
```

From `tests/ui/check/warmup-not-int.atls`.

## Definedness

### `maybe-undefined`

A value that may be undefined given to an `int` or `bool`, which are always defined; test it with `defined` first.

```
error[maybe-undefined]: `up` is `bool`, which is always defined, but this value may be undefined
 --> tests/ui/check/maybe-undefined-int.atls:8:8
  |
8 |   up = move > 1;
  |        ^^^^^^^^ may be undefined
  = test it first: `defined(x) ? x > 1 : false`, or assign inside `if (defined(x)) { ... }`
```

From `tests/ui/check/maybe-undefined-int.atls`.

## Series

### `series-reads-state`

A series reads `state`; series are computed before any handler.

```
error[series-reads-state]: series cannot read state `peak`
 --> tests/ui/check/series-reads-state.atls:5:27
  |
2 |   real peak = 0;
  |        ---- declared in `state {}`
5 |   real drawdown = close / peak;
  |                           ^^^^ state is written by handlers
  = series are computed from bars and params only, before any handler runs
```

From `tests/ui/check/series-reads-state.atls`.

### `series-reads-held`

A series reads `held`; the position is read in handlers.

```
error[series-reads-held]: series cannot read `held`
 --> tests/ui/check/series-reads-held.atls:2:12
  |
2 |   real h = held;
  |            ^^^^ the position is read in handlers
  = series are computed from bars and params only
```

From `tests/ui/check/series-reads-held.atls`.

### `keep-in-series`

`keep` in a series; it only means something to `target`.

```
error[keep-in-series]: `keep` is a handler value
 --> tests/ui/check/keep-in-series.atls:2:12
  |
2 |   real k = keep;
  |            ^^^^ not a series
```

From `tests/ui/check/keep-in-series.atls`.

### `function-as-value`

A function or indicator named without its arguments.

```
error[function-as-value]: `f` is called with arguments
 --> tests/ui/check/function-as-value.atls:4:17
  |
2 |   real f(real x) { return x; }
  |        - declared here
4 | on_bar { target(f); }
  |                 ^ not a value
```

From `tests/ui/check/function-as-value.atls`.

### `named-argument-outside-call`

`name = value` where no named argument is taken.

```
error[named-argument-outside-call]: `x = ...` outside a call's arguments
 --> tests/ui/check/named-argument-outside-call.atls:3:18
  |
3 |   target(xs_mean(x = close));
  |                  ^^^^^^^^^ not an expression
```

From `tests/ui/check/named-argument-outside-call.atls`.

## Built-in functions

### `unknown-function`

A call of a function that does not exist.

```
error[unknown-function]: unknown function `cube`
 --> tests/ui/check/unknown-function.atls:2:10
  |
2 |   target(cube(close));
  |          ^^^^ not a function
  = functions: abs min max defined sign log sqrt round floor; indicators (series only): sma ema rsi lag ts_max ts_min ts_sum stdev zscore ols_slope ols_intercept kf_beta kf_error kf_sd; cross-sectional (universe handlers): xs_mean xs_sum xs_count xs_rank
```

From `tests/ui/check/unknown-function.atls`.

### `function-arity`

A function called with the wrong number of arguments.

```
error[function-arity]: `abs` takes 1 argument(s)
 --> tests/ui/check/function-arity.atls:2:10
  |
2 |   target(abs(close, 1));
  |          ^^^^^^^^^^^^^ 2 given
```

From `tests/ui/check/function-arity.atls`.

### `target-as-value`

`target` used as a value; it is a statement.

```
error[target-as-value]: `target` is a statement
 --> tests/ui/check/target-as-value.atls:2:12
  |
2 |   real x = target(1);
  |            ^^^^^^^^^ write `target(x);` on its own in `on_bar`
```

From `tests/ui/check/target-as-value.atls`.

## Indicators, windows and lookback

### `indicator-arity`

An indicator called with the wrong number of arguments.

```
error[indicator-arity]: `sma` takes (series, window)
 --> tests/ui/check/indicator-arity.atls:2:12
  |
2 |   real a = sma(close);
  |            ^^^^^^^^^^ 1 argument(s)
```

From `tests/ui/check/indicator-arity.atls`.

### `series-not-numeric`

An indicator over a `bool` series.

```
error[series-not-numeric]: `sma` needs a real series
 --> tests/ui/check/series-not-numeric.atls:2:16
  |
2 |   real a = sma(close > 1, 5);
  |                ^^^^^^^^^ this is `bool`
  = write `c ? 1 : 0` to count a condition
```

From `tests/ui/check/series-not-numeric.atls`.

### `look-ahead`

A lag of 0 or less: it would read the current or a future bar.

```
error[look-ahead]: `lag(close, -1)` would read a future bar
 --> tests/ui/check/look-ahead.atls:2:30
  |
2 |   real tomorrow = lag(close, -1);
  |                              ^^ a negative lag looks ahead
  = a strategy only sees bars up to the current one; a lag is at least 1
```

From `tests/ui/check/look-ahead.atls`.

### `window-below-one`

A window that the parameters' bounds do not prove at least 1.

```
error[window-below-one]: `sma`'s window may be below 1
 --> tests/ui/check/window-unbounded.atls:6:23
  |
6 |   real a = sma(close, fast);
  |                       ^^^^ nothing bounds this from below
  = give the param a lower bound, e.g. `int<lower=1> n = 20;`
```

From `tests/ui/check/window-unbounded.atls`.

### `window-below-two`

A deviation's or a regression's window that the bounds do not prove at least 2.

```
error[window-below-two]: `stdev`'s window may be below 2
 --> tests/ui/check/window-below-two.atls:2:25
  |
2 |   real a = stdev(close, 1);
  |                         ^ a deviation needs two values
  = raise the params' lower bounds so the window is at least 2
```

From `tests/ui/check/window-below-two.atls`.

### `filter-parameters`

A Kalman filter's `delta` not provably in [0, 1], or `ve` not provably at least 0.

```
error[filter-parameters]: the filter needs delta in [0, 1) and ve >= 0
 --> tests/ui/check/filter-parameters.atls:5:12
  |
5 |   real b = kf_beta(close, open, delta, 0.001);
  |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ may be outside within the params' bounds
  = bound the params, e.g. `real<lower=0, upper=1> delta = 0.0001;`
```

From `tests/ui/check/filter-parameters.atls`.

### `indicator-in-handler`

An indicator (or a lag) called outside `series {}`: it must see every bar.

```
error[indicator-in-handler]: indicator `sma` called inside `on_bar`
 --> tests/ui/check/indicator-in-handler.atls:6:20
  |
6 |     target(close > sma(close, fast) ? 100 : 0);
  |                    ^^^^^^^^^^^^^^^^ indicators are computed in `series {}`
  = an indicator keeps the history of its input, so it must see every bar; under an `if` it would skip bars and change meaning
  = declare `real x = sma(close, fast);` in `series {}` and read `x` here
```

From `tests/ui/check/indicator-in-handler.atls`.

## Handlers and statements

### `assign-param`

An assignment to a parameter.

```
error[assign-param]: cannot assign to parameter `quantity`
 --> tests/ui/check/assign-param.atls:9:5
  |
2 |   int<lower=0> quantity = 100;
  |                -------- declared in `params {}`
9 |     quantity = 50;
  |     ^^^^^^^^ params are fixed for a whole run
  = a value that changes during the run belongs in `state {}`
```

From `tests/ui/check/assign-param.atls`.

### `assign-series`

An assignment to a series.

```
error[assign-series]: cannot assign to series `average`
 --> tests/ui/check/assign-series.atls:5:3
  |
2 |   real average = sma(close, 20);
  |        ------- declared in `series {}`
5 |   average = close;
  |   ^^^^^^^ series are computed from the bars, every bar
  = a value a handler changes belongs in `state {}`
```

From `tests/ui/check/assign-series.atls`.

### `assign-instrument`

An assignment to an instrument's name.

```
error[assign-instrument]: cannot assign to instrument `y`
 --> tests/ui/check/assign-instrument.atls:3:3
  |
3 |   y = 1;
  |   ^ instruments are inputs
```

From `tests/ui/check/assign-instrument.atls`.

### `assign-function`

An assignment to a function's or indicator's name.

```
error[assign-function]: cannot assign to `f`
 --> tests/ui/check/assign-function.atls:5:3
  |
5 |   f = 1;
  |   ^ a function or indicator
```

From `tests/ui/check/assign-function.atls`.

### `assign-local`

An assignment to a local; locals are fixed once declared.

```
error[assign-local]: cannot assign to local `x`
 --> tests/ui/check/assign-local.atls:3:3
  |
3 |   x = 2;
  |   ^ locals are fixed once declared
  = declare a new local, or use `state {}` for a value that changes
```

From `tests/ui/check/assign-local.atls`.

### `assign-read-only`

An assignment to a bar field or `held`.

```
error[assign-read-only]: cannot assign to `close`
 --> tests/ui/check/assign-read-only.atls:2:3
  |
2 |   close = 1;
  |   ^^^^^ read-only
```

From `tests/ui/check/assign-read-only.atls`.

### `target-in-handler`

`target` outside `on_bar`, `on_open` and `on_close`.

```
error[target-in-handler]: `target` in `on_fill`
 --> tests/ui/check/target-in-on-fill.atls:5:3
  |
5 |   target(0);
  |   ^^^^^^^^^^ targets are stated in `on_bar` (or `on_open`, `on_close`) only
  = record what you need in `state {}` and act on the next bar
```

From `tests/ui/check/target-in-on-fill.atls`.

### `target-not-numeric`

A target that is not a number of shares.

```
error[target-not-numeric]: a target is a number of shares
 --> tests/ui/check/target-not-numeric.atls:2:10
  |
2 |   target(close > 1);
  |          ^^^^^^^^^ this is `bool`
```

From `tests/ui/check/target-not-numeric.atls`.

### `return-outside-function`

`return` outside a function of `functions {}`.

```
error[return-outside-function]: `return` outside a function
 --> tests/ui/check/return-in-handler.atls:2:3
  |
2 |   return 1;
  |   ^^^^^^^^^ not here
  = handlers state targets; only functions of `functions {}` return a value
```

From `tests/ui/check/return-in-handler.atls`.

### `while-loop`

A `while` loop; every loop is a bounded `for`.

```
error[while-loop]: `while` loops are not allowed
 --> tests/ui/check/unbounded-loop.atls:5:3
  |
5 |   while (n < 10) {
  |   ^^^^^ may never end
  = every handler must finish: use `for (i in FROM:TO)` with bounds made of numbers and params
```

From `tests/ui/check/unbounded-loop.atls`.

### `expression-statement`

An expression used as a statement; its value would be unused.

```
error[expression-statement]: this statement does nothing
 --> tests/ui/check/expression-statement.atls:2:3
  |
2 |   close + 1;
  |   ^^^^^^^^^ its value is unused
  = a handler assigns state, declares locals, branches, loops, or states `target(...)`
```

From `tests/ui/check/expression-statement.atls`.

### `no-bar`

A bar field or a series read where there is no bar (`on_start`, `on_stop`).

```
error[no-bar]: `close` has no bar to read in `on_start`
 --> tests/ui/check/no-bar.atls:5:11
  |
5 |   first = close;
  |           ^^^^^ no bar here
```

From `tests/ui/check/no-bar.atls`.

### `held-in-handler`

`held` (or `y.held`) read in a handler that has no position to read.

```
error[held-in-handler]: `held` is read in `on_bar`, not in `on_fill`
 --> tests/ui/check/held-in-handler.atls:6:7
  |
6 |   h = held;
  |       ^^^^ not here
```

From `tests/ui/check/held-in-handler.atls`.

### `fill-outside-on-fill`

`fill.*` read outside `on_fill`.

```
error[fill-outside-on-fill]: `fill` is read in `on_fill`, not in `on_bar`
 --> tests/ui/check/fill-outside-on-fill.atls:2:10
  |
2 |   target(fill.price);
  |          ^^^^ no fill here
```

From `tests/ui/check/fill-outside-on-fill.atls`.

### `fill-without-field`

`fill` read without a field.

```
error[fill-without-field]: `fill` has fields
 --> tests/ui/check/fill-without-field.atls:6:7
  |
6 |   f = fill;
  |       ^^^^ write `fill.price`, `fill.quantity` or `fill.buy`
```

From `tests/ui/check/fill-without-field.atls`.

### `unknown-field`

A field that `fill` or an instrument does not have.

```
error[unknown-field]: `fill` has no field `time`
 --> tests/ui/check/unknown-field.atls:6:12
  |
6 |   f = fill.time;
  |            ^^^^ fields: price, quantity, buy
```

From `tests/ui/check/unknown-field.atls`.

### `no-fields`

A field read from a name that has none.

```
error[no-fields]: `close` has no fields
 --> tests/ui/check/no-fields.atls:2:10
  |
2 |   target(close.price);
  |          ^^^^^^^^^^^ only `fill.*` has fields
```

From `tests/ui/check/no-fields.atls`.

### `fields-in-series`

`fill.*` (or another non-instrument field) read in `series {}`.

```
error[fields-in-series]: `fill` is not available in `series {}`
 --> tests/ui/check/fields-in-series.atls:2:12
  |
2 |   real p = fill.price;
  |            ^^^^^^^^^^ no fields here
  = `fill.*` is read in `on_fill`
```

From `tests/ui/check/fields-in-series.atls`.

## Instruments and universes

### `empty-instruments`

`instruments {}` that names no instrument.

```
error[empty-instruments]: `instruments {}` names no instrument
 --> tests/ui/check/empty-instruments.atls:1:1
  |
1 | instruments {}
  | ^^^^^^^^^^^^^^ empty
  = write `instruments { y; x; }`
```

From `tests/ui/check/empty-instruments.atls`.

### `instruments-and-universe`

A program with both `instruments {}` and `universe {}`.

```
error[instruments-and-universe]: a program is either `instruments {}` or `universe {}`
 --> tests/ui/check/instruments-and-universe.atls:2:1
  |
1 | instruments { y; }
  | ------------------ first
2 | universe {}
  | ^^^^^^^^^^^ second
  = named instruments are read as `y.close`; a universe runs the same code for every instrument
```

From `tests/ui/check/instruments-and-universe.atls`.

### `instrument-as-value`

An instrument's name used as a value; read one of its fields.

```
error[instrument-as-value]: `y` is an instrument
 --> tests/ui/check/instrument-as-value.atls:3:13
  |
3 |   target(y, y);
  |             ^ read one of its fields
  = write `y.close` or `y.held`
```

From `tests/ui/check/instrument-as-value.atls`.

### `bare-field`

A bare bar field (`close`) in a program that names its instruments.

```
error[bare-field]: which instrument's `close`?
 --> tests/ui/check/named-bare-field.atls:2:24
  |
2 | series { real spread = close - x.close; }
  |                        ^^^^^ the program names its instruments
  = write `y.close`
```

From `tests/ui/check/named-bare-field.atls`.

### `not-a-series-field`

An instrument field that is not a series (`y.held`, an unknown field) read in `series {}`.

```
error[not-a-series-field]: `y.held` is not a series
 --> tests/ui/check/not-a-series-field.atls:3:14
  |
3 |   real h = y.held;
  |              ^^^^ fields: open, high, low, close, volume
  = an instrument's position, `y.held`, is read in handlers
```

From `tests/ui/check/not-a-series-field.atls`.

### `held-needs-instrument`

A bare `held` in a program that names its instruments.

```
error[held-needs-instrument]: whose position?
 --> tests/ui/check/held-needs-instrument.atls:3:13
  |
3 |   target(y, held);
  |             ^^^^ the program names its instruments
  = write `y.held`
```

From `tests/ui/check/held-needs-instrument.atls`.

### `not-an-instrument`

`target(name, ...)` with a name that is not in `instruments {}`.

```
error[not-an-instrument]: `x` is not an instrument
 --> tests/ui/check/not-an-instrument.atls:3:10
  |
3 |   target(x, 1);
  |          ^ not in `instruments {}`
```

From `tests/ui/check/not-an-instrument.atls`.

### `target-needs-instrument`

`target(shares)` in a program that names its instruments.

```
error[target-needs-instrument]: which instrument's target?
 --> tests/ui/check/target-needs-instrument.atls:3:10
  |
3 | on_bar { target(r > 1 ? 100 : 0); }
  |          ^^^^^^^^^^^^^^^^^^^^^^^^ the program names its instruments
  = write `target(y, shares);`
```

From `tests/ui/check/target-needs-instrument.atls`.

### `target-names-instrument`

`target(name, shares)` in a program without `instruments {}`.

```
error[target-names-instrument]: `target(name, shares)` needs `instruments {}`
 --> tests/ui/check/target-names-instrument.atls:2:10
  |
2 |   target(y, 1);
  |          ^ no named instruments
  = write `target(shares);`
```

From `tests/ui/check/target-names-instrument.atls`.

### `bars-outside-universe`

`bars` or `periods` outside a universe's handlers.

```
error[bars-outside-universe]: `bars` is read in a universe's handlers
 --> tests/ui/check/bars-outside-universe.atls:2:10
  |
2 |   target(bars);
  |          ^^^^ not here
  = `bars` counts this instrument's bars, `periods` every period; `bars == periods` while it has missed none
```

From `tests/ui/check/bars-outside-universe.atls`.

### `xs-in-series`

A cross-sectional operator in `series {}`; it is computed in handlers.

```
error[xs-in-series]: `xs_mean` is computed in handlers
 --> tests/ui/check/xs-in-series.atls:3:12
  |
3 |   real m = xs_mean(close);
  |            ^^^^^^^^^^^^^^ not in `series {}`
  = a cross-sectional value needs every instrument's series first; compute it at the top of `on_bar` (or `on_open`/`on_close`)
```

From `tests/ui/check/xs-in-series.atls`.

### `xs-outside-universe`

A cross-sectional operator in a program without `universe {}`.

```
error[xs-outside-universe]: `xs_mean` needs `universe {}`
 --> tests/ui/check/xs-in-single.atls:2:23
  |
2 | on_bar { target(ret > xs_mean(ret) ? 100 : 0); }
  |                       ^^^^^^^^^^^^ no cross-section here
  = a cross-sectional operator combines every instrument of a universe at one moment
```

From `tests/ui/check/xs-in-single.atls`.

### `xs-nested`

A cross-sectional operator under `if` or `for`; it must see every instrument.

```
error[xs-nested]: `xs_mean` inside `if` or `for`
 --> tests/ui/check/xs-under-if.atls:5:18
  |
5 |     target(ret - xs_mean(ret) > 0 ? 100 : 0);
  |                  ^^^^^^^^^^^^ must see every instrument
  = a cross-sectional value combines every instrument, so it cannot depend on one instrument's branch
  = declare `real m = xs_mean(ret);` at the top of `on_bar` and read `m` here
```

From `tests/ui/check/xs-under-if.atls`.

### `bar-and-legs`

`on_bar` together with `on_open` or `on_close`.

```
error[bar-and-legs]: `on_bar` and `on_open`/`on_close` in one program
 --> tests/ui/check/bar-and-legs.atls:2:1
  |
2 | on_bar { target(1); }
  | ^^^^^^ `on_bar`
3 | on_close { target(0); }
  | -------- a session leg
  = a program sees every bar (`on_bar`) or each day as an open and a close (`on_open`, `on_close`), not both
```

From `tests/ui/check/bar-and-legs.atls`.

### `legs-outside-universe`

`on_open` or `on_close` in a program that is not a universe.

```
error[legs-outside-universe]: `on_open`/`on_close` are supported in `universe {}` programs only
 --> tests/ui/check/legs-outside-universe.atls:1:1
  |
1 | on_open {
  | ^^^^^^^ not here yet
  = add `universe {}`; a single-instrument program with legs is not built yet
```

From `tests/ui/check/legs-outside-universe.atls`.

### `open-reads-future`

Today's `close`, `high`, `low` or `volume` read in `on_open`.

```
error[open-reads-future]: today's `close` is not known at the open
 --> tests/ui/check/open-reads-close.atls:4:21
  |
4 |   real gap = open / close - 1;
  |                     ^^^^^ would read the future
  = in `on_open` only `open` is today's; series hold the last close's values
  = declare `real last_close = close;` in `series {}` to read the last close's close
```

From `tests/ui/check/open-reads-close.atls`.

### `on-fill-outside-single`

`on_fill` in a program with named instruments or a universe.

```
error[on-fill-outside-single]: `on_fill` is supported in single-instrument programs only
 --> tests/ui/check/on-fill-outside-single.atls:3:1
  |
3 | on_fill { }
  | ^^^^^^^ not here yet
```

From `tests/ui/check/on-fill-outside-single.atls`.

### `universe-lifecycle-handler`

`on_start` or `on_stop` in a universe program (the prototype accepted them but generated nothing).

```
error[universe-lifecycle-handler]: `on_start` is not supported in `universe {}` programs
 --> tests/ui/check/universe-lifecycle-handler.atls:6:1
  |
6 | on_start {
  | ^^^^^^^^ not here yet
  = a universe runs its handlers per instrument; start and stop callbacks per instrument are not built yet
```

From `tests/ui/check/universe-lifecycle-handler.atls`.

## Missing-value policies

### `unknown-option`

An option other than `missing` in `options {}`.

```
error[unknown-option]: unknown option `fill`
 --> tests/ui/check/unknown-option.atls:2:3
  |
2 |   fill = next;
  |   ^^^^ not an option
  = options: missing = restart | skip | poison
```

From `tests/ui/check/unknown-option.atls`.

### `unknown-policy`

A missing-value policy other than `restart`, `skip` or `poison`.

```
error[unknown-policy]: `drop` is not a missing-value policy
 --> tests/ui/check/policy-unknown.atls:1:21
  |
1 | options { missing = drop; }
  |                     ^^^^ expected restart, skip or poison
  = restart: the window starts again after a missing value; skip: a missing value is left out of the window's result; poison: undefined for good from the first missing value
```

From `tests/ui/check/policy-unknown.atls`.

### `unknown-named-argument`

An indicator called with a named argument it does not take.

```
error[unknown-named-argument]: `sma` takes no argument `window`
 --> tests/ui/check/unknown-named-argument.atls:2:26
  |
2 |   real a = sma(close, 5, window = 2);
  |                          ^^^^^^ unknown argument
  = named arguments: missing = restart | skip | poison, and min = N with skip
```

From `tests/ui/check/unknown-named-argument.atls`.

### `named-argument-not-allowed`

A named argument given to a function or indicator that takes none.

```
error[named-argument-not-allowed]: `abs` takes no named argument
 --> tests/ui/check/named-argument-not-allowed.atls:2:23
  |
2 |   real a = abs(close, missing = skip);
  |                       ^^^^^^^ not here
  = only indicators over one series take `missing = ...` (sma ema lag ts_sum stdev stdev_pop zscore, and rsi ts_max ts_min with restart or poison)
```

From `tests/ui/check/named-argument-not-allowed.atls`.

### `min-without-skip`

`min =` with a policy other than `skip`.

```
error[min-without-skip]: `min` applies to `missing = skip` only
 --> tests/ui/check/policy-min-restart.atls:1:40
  |
1 | series { real m = sma(close, 20, min = 5); }
  |                                        ^ not used
  = restart and poison need a full window of values
```

From `tests/ui/check/policy-min-restart.atls`.

### `min-not-applicable`

`min =` on `lag` or `ema`, where it would have no effect (the prototype accepted and ignored it).

```
error[min-not-applicable]: `lag` takes no `min`
 --> tests/ui/check/min-not-applicable.atls:3:55
  |
3 |   real previous = lag(close, 3, missing = skip, min = 2);
  |                                                       ^ it would have no effect
  = under `skip`, a lag reads the value in that exact slot, which may be missing
```

From `tests/ui/check/min-not-applicable.atls`.

### `min-out-of-range`

A minimum count (`min =`, or `ts_sum`'s third argument) not provably between 1 and the window.

```
error[min-out-of-range]: `min` must be between 1 and the window
 --> tests/ui/check/min-out-of-range.atls:2:48
  |
2 |   real a = sma(close, 5, missing = skip, min = 6);
  |                                                ^ may be outside within the params' bounds
```

From `tests/ui/check/min-out-of-range.atls`.

### `skip-unsupported`

`missing = skip` on an indicator without a skip rule (`rsi`, `ts_max`, `ts_min`).

```
error[skip-unsupported]: `rsi` has no `skip` rule yet
 --> tests/ui/check/policy-skip-rsi.atls:1:19
  |
1 | series { real r = rsi(close, 3, missing = skip); }
  |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ restart or poison only
  = write `missing = restart` or `missing = poison` in the call
```

From `tests/ui/check/policy-skip-rsi.atls`.

## Functions and user indicators

### `duplicate-argument`

A function with two arguments of one name.

```
error[duplicate-argument]: argument `x` twice
 --> tests/ui/check/duplicate-argument.atls:2:23
  |
2 |   real f(real x, real x) { return x; }
  |                       ^ second
```

From `tests/ui/check/duplicate-argument.atls`.

### `int-function-real-arguments`

A function returning `int` with an argument that is not `int`.

```
error[int-function-real-arguments]: a function returning `int` takes `int` arguments only
 --> tests/ui/check/int-function-real-arguments.atls:2:7
  |
2 |   int f(real x) { return 1; }
  |       ^ here
  = an undefined real argument would make an undefined `int`; return `real` and use `round` or `floor`
```

From `tests/ui/check/int-function-real-arguments.atls`.

### `missing-return`

A function with a path that does not end in `return`.

```
error[missing-return]: `clip` may end without returning
 --> tests/ui/check/function-no-return.atls:2:8
  |
2 |   real clip(real z, real cap) {
  |        ^^^^ every path must end in `return`
  = end the body with `return x;`, or end both branches of the last `if`/`else` with one
```

From `tests/ui/check/function-no-return.atls`.

### `not-visible`

A name a function or indicator body cannot see: it sees only its own arguments, locals, states and values.

```
error[not-visible]: `close` is not visible in `gap`
 --> tests/ui/check/function-reads-bar.atls:3:16
  |
3 |     return x - close;
  |                ^^^^^ not here
  = a function sees only its arguments and locals
  = pass `close` as an argument
```

From `tests/ui/check/function-reads-bar.atls`.

### `assign-in-function`

An assignment, in a function or indicator, to something that is not one of its states.

```
error[assign-in-function]: cannot assign to `x` in `f`
 --> tests/ui/check/assign-in-function.atls:3:5
  |
3 |     x = 1;
  |     ^ not a state of this indicator
  = a function is pure: it declares locals and returns a value
```

From `tests/ui/check/assign-in-function.atls`.

### `assign-state-outside-step`

An indicator state assigned outside `update`, `enter` and `leave`.
Not reachable today: assignments only occur in those and in functions.

### `loop-bounds-not-int`

A loop in a function or indicator whose bounds are not `int`.

```
error[loop-bounds-not-int]: a loop's bounds are whole numbers
 --> tests/ui/check/loop-bounds-not-int.atls:3:15
  |
3 |     for (i in 1:x) { }
  |               ^^^ not `int`
```

From `tests/ui/check/loop-bounds-not-int.atls`.

### `call-in-body`

A cross-sectional operator or a user indicator called inside a function or indicator.

```
error[call-in-body]: `xs_mean` inside `f`
 --> tests/ui/check/call-in-body.atls:2:27
  |
2 |   real f(real x) { return xs_mean(x); }
  |                           ^^^^^^^^^^ not here
  = functions and indicators see only their own arguments: indicators and cross-sectional values are used in a program's series and handlers
```

From `tests/ui/check/call-in-body.atls`.

### `field-in-body`

A field other than the window's `count` read inside a function or indicator.

```
error[field-in-body]: `w.size` is not available in `f`
 --> tests/ui/check/field-in-body.atls:3:18
  |
3 |   value real v = w.size;
  |                  ^^^^^^ not here
  = an indicator reads its window as `w.count`; nothing else has fields here
```

From `tests/ui/check/field-in-body.atls`.

### `indicator-bool-argument`

An indicator argument declared `bool`.

```
error[indicator-bool-argument]: an indicator's arguments are `real` (series) or `int` (fixed)
 --> tests/ui/check/indicator-bool-argument.atls:1:26
  |
1 | indicator f(real x, bool b) {
  |                          ^ a `bool`
  = pass a condition as a series: `c ? 1 : 0`
```

From `tests/ui/check/indicator-bool-argument.atls`.

### `indicator-without-series`

An indicator with no `real` (series) argument.

```
error[indicator-without-series]: `f` takes no series
 --> tests/ui/check/indicator-without-series.atls:1:11
  |
1 | indicator f(int n) {
  |           ^ no `real` argument
  = an indicator follows at least one series; a computation on fixed values is a function
```

From `tests/ui/check/indicator-without-series.atls`.

### `duplicate-window`

An indicator with two windows.

```
error[duplicate-window]: a second window
 --> tests/ui/check/duplicate-window.atls:3:3
  |
3 |   window u[4];
  |   ^^^^^^^^^^^^ one window per indicator
  = the window holds every series argument, so one is enough
```

From `tests/ui/check/duplicate-window.atls`.

### `window-not-int`

An indicator's window length that is not a whole number.

```
error[window-not-int]: a window's length is a whole number
 --> tests/ui/check/window-not-int.atls:2:12
  |
2 |   window w[2.5];
  |            ^^^ this is `real`
```

From `tests/ui/check/window-not-int.atls`.

### `window-not-constant`

An indicator's window length that is not built from numbers and `int` arguments with `+ - *`.

```
error[window-not-constant]: a window's length is built from numbers and int arguments with + - *
 --> tests/ui/check/window-not-constant.atls:2:12
  |
2 |   window w[abs(n)];
  |            ^^^^^^ not that
```

From `tests/ui/check/window-not-constant.atls`.

### `step-without-window`

`enter` or `leave` in an indicator without a window.

```
error[step-without-window]: `enter` without a window
 --> tests/ui/check/step-without-window.atls:3:3
  |
3 |   enter(v) { s = s + v; }
  |   ^^^^^^^^^^^^^^^^^^^^^^^ nothing enters or leaves
  = declare `window w[n];`
```

From `tests/ui/check/step-without-window.atls`.

### `step-arity`

`enter` or `leave` naming a number of values other than the number of series arguments.

```
error[step-arity]: `enter` names 1 value(s); the window holds 2
 --> tests/ui/check/step-arity.atls:4:3
  |
4 |   enter(a) { s = s + a; }
  |   ^^^^^^^^^^^^^^^^^^^^^^^ one name per series argument
```

From `tests/ui/check/step-arity.atls`.

### `enter-leave-pair`

`enter` without `leave`, or the reverse.

```
error[enter-leave-pair]: `enter` and `leave` come together
 --> tests/ui/check/indicator-enter-without-leave.atls:1:11
  |
1 | indicator u_sum(real x, int n) {
  |           ^^^^^ only one of them
  = whatever `enter` adds for a value, `leave` takes back when that value leaves the window
```

From `tests/ui/check/indicator-enter-without-leave.atls`.

### `value-not-real`

An indicator `value` that is not `real`.

```
error[value-not-real]: an indicator's values are `real`
 --> tests/ui/check/value-not-real.atls:2:9
  |
2 |   value int v = 1;
  |         ^^^ write `real`
```

From `tests/ui/check/value-not-real.atls`.

### `indicator-without-value`

An indicator that reports no `value`.

```
error[indicator-without-value]: `f` reports nothing
 --> tests/ui/check/indicator-without-value.atls:1:11
  |
1 | indicator f(real x) {
  |           ^ no `value`
  = add `value real v = ...;`
```

From `tests/ui/check/indicator-without-value.atls`.

### `unknown-value`

`ind(...).v` naming a value the indicator does not have.

```
error[unknown-value]: `band` has no value `middle`
 --> tests/ui/check/unknown-value.atls:6:24
  |
6 |   real z = band(close).middle;
  |                        ^^^^^^ values: lower_band, upper_band
```

From `tests/ui/check/unknown-value.atls`.

### `ambiguous-value`

A call of an indicator with several values that does not name one.

```
error[ambiguous-value]: `band` has several values
 --> tests/ui/check/ambiguous-value.atls:6:12
  |
6 |   real z = band(close);
  |            ^^^^^^^^^^^ which one?
  = write `band(...).lower_band`
```

From `tests/ui/check/ambiguous-value.atls`.

### `member-not-indicator`

`.name` after a call of something that is not a user indicator.

```
error[member-not-indicator]: `.v` reads a value of a user indicator
 --> tests/ui/check/member-not-indicator.atls:5:12
  |
5 |   real z = f(close).v;
  |            ^^^^^^^^^^ not one
  = write `name(args).value` for an indicator with several values
```

From `tests/ui/check/member-not-indicator.atls`.

### `user-indicator-in-handler`

A value of a user indicator read in a handler; it is computed in `series {}`.

```
error[user-indicator-in-handler]: a user indicator is read in `series {}`
 --> tests/ui/check/user-indicator-in-handler.atls:6:10
  |
6 |   target(band(close).upper_band);
  |          ^^^^^^^^^^^^^^^^^^^^^^ not here
  = an indicator keeps the history of its input, so it must see every bar; under an `if` it would skip bars and change meaning
```

From `tests/ui/check/user-indicator-in-handler.atls`.
