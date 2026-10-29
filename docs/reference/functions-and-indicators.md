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
