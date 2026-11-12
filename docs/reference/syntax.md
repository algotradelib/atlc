# Syntax

## Lexical rules

- A program is UTF-8 text; files are conventionally named `*.atls` (the compiler does not require it).
- Comments run from `#` or `//` to the end of the line.
- Names are ASCII letters, digits and `_`, not starting with a digit.
- A string `"..."` is read as one token, but no rule takes one.
- Numbers are decimal: `20`, `0.05`, `1e-4`.
  An `int` literal has no `.` and no exponent.
- Keywords are reserved everywhere: `params series state options functions instruments universe data replay warmup on_start on_bar on_fill on_stop on_open on_close int real bool true false if else for in return target and or not indicator while`.
  `while` is reserved only so that the compiler can explain why it is refused: every loop is a bounded `for`.
  `replay` is reserved for the same reason: a program holds no run settings, so `replay {}` is no block.
  `warmup` begins an indicator's part (`warmup n;`) and is the one keyword that is also an option's name (`options { warmup = n + 1; }`); it is no block.
- `window value update enter leave` begin the parts of an `indicator` (`window w[n];`, `value real v = ...;`).
  They are not reserved: they are ordinary names everywhere, inside an indicator too (`indicator f(real window)` is valid).
- `extern` begins an external indicator at the top level (`extern indicator f(...) { ... }`) and an external function in `functions {}` ([functions and indicators](functions-and-indicators.md), "External C++").
  It is not reserved either: it is an ordinary name everywhere else.
- `lower` and `upper` are not keywords, but they are reserved as names: no declaration may use them ([program structure](program-structure.md), "Names").

## Grammar

EBNF: `'x'` is a literal, `A*` zero or more, `A?` optional, `|` a choice.
This is the grammar of valid programs.
The parser also accepts a few forms only to report a better error later (an expression used as a statement, `while`, `target` used as a value, as in `x = target(1);`); those are diagnostics, not part of the language.
A program may nest at most 256 levels deep: blocks, `else if`, parentheses and every level of an expression's tree count, and a chain of operators deepens its tree by one level per operator (`1 + 1 + 1` is three levels).
Deeper programs are refused (`nesting-too-deep`): an implementation limit, not part of the grammar.

```ebnf
program    = item* ;
item       = 'params' '{' param* '}'
           | 'series' '{' decl* '}'
           | 'state' '{' decl* '}'
           | 'options' '{' option* '}'
           | 'instruments' '{' ( NAME ';' )* '}'
           | 'universe' '{' '}'
           | 'data' '{' data_decl* '}'
           | 'functions' '{' function* '}'
           | 'indicator' NAME args '{' part* '}'
           | 'extern' 'indicator' NAME args '{' extern_part* '}'
           | handler block ;
handler    = 'on_start' | 'on_bar' | 'on_fill' | 'on_stop' | 'on_open' | 'on_close' ;

data_decl  = type NAME ';' ;                              (* the checker requires `real` *)
param      = type bounds? NAME '=' expr ';' ;            (* the checker requires a literal, parentheses allowed *)
bounds     = '<' bound ( ',' bound )* '>' ;
bound      = ( 'lower' | 'upper' ) '=' additive ;        (* stops before a comparison, so '>' closes the bounds *)
decl       = type NAME '=' expr ';' ;
type       = 'int' | 'real' | 'bool' ;
option     = ( NAME | 'warmup' ) '=' expr ';' ;            (* `warmup` is the one keyword that is also an option *)

function   = type NAME args block
           | 'extern' type NAME args ';' ;
args       = '(' ( type NAME ( ',' type NAME )* )? ')' ;
part       = 'window' NAME '[' expr ']' ';'
           | 'state' decl
           | 'value' decl
           | 'warmup' expr ';'
           | 'update' block
           | ( 'enter' | 'leave' ) '(' ( NAME ( ',' NAME )* )? ')' block ;   (* the checker requires one name per real input *)
extern_part = 'warmup' expr ';'
           | 'value' type NAME ';' ;                    (* the checker requires `real` *)

block      = '{' stmt* '}' ;
stmt       = decl
           | NAME '=' expr ';'
           | 'target' '(' ( NAME ',' )? expr ')' ';'
           | if_stmt
           | 'for' '(' NAME 'in' additive ':' additive ')' block
           | 'return' expr ';' ;
if_stmt    = 'if' '(' expr ')' block ( 'else' ( if_stmt | block ) )? ;

expr       = or ( '?' expr ':' expr )? ;
or         = and ( 'or' and )* ;
and        = not ( 'and' not )* ;
not        = 'not' not | comparison ;
comparison = additive ( ( '<' | '<=' | '>' | '>=' | '==' | '!=' ) additive )? ;   (* does not chain *)
additive   = term ( ( '+' | '-' ) term )* ;
term       = unary ( ( '*' | '/' ) unary )* ;
unary      = '-' unary | postfix ;
postfix    = primary ( '[' expr ']' )* ;                  (* x[k] is lag(x, k) *)
primary    = NUMBER | 'true' | 'false'
           | NAME
           | NAME '(' ( arg ( ',' arg )* )? ')' ( '.' NAME )?
           | NAME '.' NAME
           | '(' expr ')' ;
arg        = expr | NAME '=' expr ;                        (* named: missing = skip, min = 5 *)
```

## Notes

- `NAME '.' NAME` is `fill.price` in `on_fill`, and `y.close` or `y.held` for an instrument named in `instruments {}`.
- `NAME(...).NAME` reads one value of a user indicator that has several (`band(close, 20).upper`).
- `target` is a statement, not a function: `real y = target(1) + 1;` is refused (`target-as-value`).
- Which statements and expressions are allowed in which block is not part of the grammar; [program structure](program-structure.md) states it.
