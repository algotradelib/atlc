# The atls language reference

Draft, for `atlc 0.0.0-9000`; until the first release the language has no version of its own.

This reference defines the language.
Where it and `atlc` disagree, the reference states the intended behaviour and `atlc` has a bug, unless the reference says a behaviour is unspecified.
The grammar in [syntax.md](syntax.md) is checked against the compiler's parser by the test suite (`grammar/`, an LR(1) copy for LALRPOP, which refuses an ambiguous grammar): both accept and refuse the same programs at the syntax stage and build the same tree.

| Chapter | Contents |
| --- | --- |
| [Syntax](syntax.md) | Lexical rules and the grammar |
| [Program structure](program-structure.md) | The blocks, the order they run in, warm-up and lookback |
| [Types and definedness](types-and-definedness.md) | `int`, `real`, `bool`; undefined values; `defined` and narrowing |
| [Instruments](instruments.md) | One instrument, named instruments, universes; cross-sectional operators; session legs |
| [Missing values](missing-values.md) | `restart`, `skip`, `poison`, and how a window treats a missing value |
| [Built-in functions and indicators](builtins.md) | Every built-in, its definition and its arithmetic |
| [Functions and indicators](functions-and-indicators.md) | `functions {}` and user-defined `indicator`s |
| [The generated C++](generated-code.md) | What `atlc build` writes, the runtime it includes, and how to compile it |
| [Run spec](run-spec.md) | What `atlc run` reads: data, replay settings, parameters, build settings |

Conventions: "must" and "must not" are rules the checker enforces; "is undefined" means a value is NaN in the sense of [types and definedness](types-and-definedness.md); file names are relative to the repository.
Every refusal and warning has a stable code, listed in [diagnostics](../diagnostics.md).
