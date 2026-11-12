# Functions and indicators

## `functions {}`

```atls
functions {
  real clip(real z, real cap) {
    return min(max(z, -cap), cap);
  }
}
```

- A function is typed and pure: it sees only its arguments and its locals.
- Every path must end in `return`; loops are bounded `for`s; a function may call only functions declared above it (no recursion).
- A function that returns `int` takes `int` arguments only.
- A call is undefined, without running the body, when a `real` or `bool` argument is undefined.
- Functions may be called from `series {}`, from handlers and from indicator bodies.

## `indicator`

```atls
indicator mean_sd(real x, int n) {
  window w[n];
  state real s = 0;
  state real q = 0;
  enter(v) { s = s + v; q = q + v * v; }
  leave(v) { s = s - v; q = q - v * v; }
  value real mean = s / w.count;
  value real sd = sqrt((q - s * s / w.count) / (w.count - 1));
}
```

- `real` arguments are series; `int` arguments are fixed at the call.
- At most one `window w[n];`.
  It holds the last n tuples of every `real` argument, and carries the missing-value policy: a tuple is missing when any member is.
- `enter(...)` and `leave(...)` appear together and take one name per `real` argument; `enter` runs when a tuple comes in, then `leave` for the one going out; neither is given a missing tuple.
- `update {}` runs on each period with a valid input, before the window takes the new tuple; it may be used with or without a window (an EMA needs no window).
  On a missing input it does not run: under `restart` the indicator resets, under `poison` it stops, and under `skip` the window still advances: the missing tuple takes a slot, a valid tuple may leave (running `leave`) and the values are computed again; only `update` and `enter` are passed over.
- `value` lines are the outputs (`real` only), computed after `enter` and `leave`; with several, a caller reads one as `mean_sd(close, 20).sd`.
  Since `skip` computes them again on a step whose input is missing, a `real` argument read in a value may be undefined there, as in a handler: a condition on it is undefined, and `defined(x)` tests it.
- `warmup n;` states the periods before the first value; `w.count` is the number of valid (not missing) tuples in the window.
- An indicator may be called only in `series {}`; its lookback and definedness are inferred, with the checks that apply to built-ins.
- Identical calls share one instance.

Not available yet: arrays, `w.ago(k)`, windows of derived values, and an include for a prelude of indicators written in atls.

## External C++

As in Stan, a program may declare a function or an indicator whose body is the user's C++: the program declares it with `extern`, and whoever compiles the generated header supplies a header that defines it ([the generated C++](generated-code.md), "External C++").
The compiler does not read that C++: `atlc check` and `atlc build` need no header and take no option for it.
An external may restate a built-in under another name (built-in names stay reserved), or compute what atls cannot write.

```atls
instruments { y; x; }

functions {
  extern real clip(real z, real cap);
}

extern indicator ols(real y, real x, int n) {
  warmup n;
  value real slope;
  value real intercept;
}

series {
  real beta = ols(y.close, x.close, 60).slope;
  real z = zscore(y.close - beta * x.close, 20);
}

on_bar {
  target(y, round(-10 * clip(z, 2)));
}
```

```cpp
namespace atls::user {

inline double clip(double z, double cap) {
  return std::min(std::max(z, -cap), cap);
}

class ols {
public:
  explicit ols(std::int64_t n);
  void update(double y, double x);
  double slope();
  double intercept();
};

} // namespace atls::user
```

### External functions

- `extern TYPE NAME(args);` in `functions {}`, without a body, declares the C++ function `atls::user::NAME`, taking and returning `double` for `real`, `std::int64_t` for `int` and `bool` for `bool`.
- The rules of a function call hold: every argument is evaluated, an undefined `real` or `bool` argument makes the call undefined without calling the C++, and a function returning `int` takes `int` arguments only.
- A `real` result may be undefined, since the C++ may return NaN; an `int` or `bool` result is defined whenever the arguments are.
- An external function may be called wherever a function may, by a later function too.

### External indicators

- `extern indicator NAME(args) { ... }` declares the C++ class `atls::user::NAME`, to be defined with the key `class`.
  Its body holds only `value real V;` lines (at least one), without an expression, and at most one `warmup EXPR;`.
- The class provides:
  - `explicit NAME(std::int64_t...)`: a constructor taking the `int` arguments in order (a default constructor when there are none);
  - `void update(double...)`: the `real` arguments in order, once per period, never an undefined value;
  - `double V()`: one accessor per value, read after each `update` once the warm-up has passed (another arithmetic return type is converted to `double`);
  - copying and moving, as a class of plain members has: a strategy is moved into its runner, and a universe copies each instrument's state when it starts.
- `real` arguments are series and `int` arguments are fixed at the call, as for an atls indicator; a fixed real is passed as a series of a parameter.
- `warmup n;` states the periods of defined input before the first value: until then every value is undefined, and the accessors are not called.
  Without it the values are read from the first period.
  The lookback and the history figure are an atls indicator's with that warm-up and no window ([program structure](program-structure.md), "Lookback and warm-up").
- Every value is "maybe undefined" to the checker, since the C++ may return NaN ([types and definedness](types-and-definedness.md)).
- Missing values ([missing values](missing-values.md)): under `restart`, an undefined input discards the object and the next defined input builds a new one from the call's `int` arguments; under `poison`, the first undefined input after the input's warm-up discards it for good.
  `skip` does not apply, since the window is in C++ (`extern-skip`): a call under a program default of `skip` must state `missing = restart` or `missing = poison`.
- Identical calls share one object, and a call's values read the same object.
- `--indicators` and the arithmetic table do not apply to user code ([types and definedness](types-and-definedness.md), "Arithmetic").
- An external's name and its values' names are used in C++ as written, so none may be a C++ keyword or a spelling C++ reserves (`__` anywhere, or `_` and an uppercase letter first), and no value may be named `update` or as its indicator (`extern-name`).

### What the compiler assumes

- The C++ is deterministic: a function's result depends only on its arguments, and an indicator's values only on its construction and the updates since.
  Sharing identical calls, the agreement of a backtest and a live run over the same bars, and identical results on one compiler and platform hold only then.
- A program with external C++ is no longer free of I/O or side effects: the compiler cannot check what the C++ does.
- An exception thrown by the C++ leaves the handler and stops the run, as an `int` overflow does ([types and definedness](types-and-definedness.md), "Types").
- Floating-point contraction in the user's C++ is the user's: the generated code turns it off only in its own functions ([the generated C++](generated-code.md), "External C++").
