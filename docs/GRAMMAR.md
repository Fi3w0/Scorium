# Scorium Grammar

This document describes the grammar the current Scorium parser accepts. It
documents what is **implemented**, not what is planned; deferred features are
listed explicitly at the end. Where this document and the parser disagree,
the parser is correct -- please open an issue.

The notation is extended-BNF-ish, written for readability rather than
formality. `{ x }` means zero or more; `[ x ]` means optional; alternatives
are separated by `|`.

---

## Lexical structure

### Whitespace and newlines

Whitespace (space, tab) and newlines separate tokens. Scorium is
newline-sensitive at the statement level: a leaf, node, variable
definition, or statement begins on a new line (or after `{`, `then`, `do`,
`else`, `elseif`).

### Comments

```
comment      ::= line_comment | block_comment
line_comment ::= '#' rest_of_line
              |  '--' rest_of_line
block_comment::= '--[[' ... ']]'
```

`#` is also the color-literal prefix, but only where a **value** is expected
(after `=`, inside a list, as a function argument). A `#` starting a token
where a statement or value is not expected begins a line comment.

Block comments do not nest.

### Identifiers and markers

```
ident      ::= ident_start { ident_continue }
ident_start::= letter | '_'
ident_continue ::= letter | digit | '_' | '-'
vardef     ::= '@' ident          # only on a definition line
interp     ::= '$' ident          # only inside a bare string
```

Identifiers may contain `-`, so `node-1`, `SUPER+Return` style tokens and
dotted bare strings (`cert.pem`) lex as single bare-string tokens in the
right context.

### Literals

```
int        ::= digit { digit }
float      ::= digit { digit } '.' digit { digit }
              |  '.' digit { digit }
bool       ::= 'true' | 'false'
nil        ::= 'nil'
color      ::= '#' hex6 | '#' hex8
hex6       ::= hex hex hex hex hex hex
hex8       ::= hex6 hex hex
duration   ::= number unit
unit       ::= 'ms' | 's' | 'm'
number     ::= int | float
quoted_str ::= '"' { char_or_escape } '"'
```

A duration without a unit is **not** a duration -- it is an integer or float.
Whether a unitless number is accepted where a duration is expected is a
schema decision (it is rejected by default).

### Operators

```
add_op   ::= '+' | '-'
mul_op   ::= '*' | '/' | '%'
rel_op   ::= '<' | '>' | '<=' | '>=' | '==' | '~='
logic_op ::= 'and' | 'or'
unary_op ::= '-' | 'not'
```

Binary operators require spaces on **both sides** inside an expression.
`base*2` is rejected by the lexer as a squeezed operator with a diagnostic
that suggests `base * 2`. This is deliberate: it stops `base*2` from being
silently read as a bare string.

---

## Syntax

### Document and items

```
document   ::= { item } { comment }

item       ::= leaf
             | node
             | vardef_stmt
             | include
             | if_stmt
             | for_stmt
             | while_stmt
             | local_stmt
             | return_stmt
             | fn_def
             | script_block
             | call_stmt
```

### Leaves and nodes

```
leaf       ::= ident '=' expr
node       ::= ident [ header ] '{' newline { item } '}'
header     ::= bare_value | quoted_str
```

A node header is a single bare token or quoted string. Its semantics are
defined by the host.

### Variables and locals

```
vardef_stmt::= vardef '=' expr          # `@name = expr`
local_stmt ::= 'local' ident '=' expr
```

`@name` defines a variable visible to later items in the same or enclosing
scope. `local name = expr` introduces a lexically scoped variable (used for
loop counters and function bodies). A `key = value` leaf whose key is already
a `local` reassigns that local in place -- this is the only spelling Scorium
has for updating a counter (see "Reassignment" below).

### Control flow

```
if_stmt    ::= 'if' expr 'then' newline { item }
               { 'elseif' expr 'then' newline { item } }
               [ 'else' newline { item } ]
               'end'
for_stmt   ::= 'for' ident '=' expr ',' expr [ ',' expr ] 'do' newline { item } 'end'
while_stmt ::= 'while' expr 'do' newline { item } 'end'
return_stmt::= 'return' [ expr ]
```

The numeric `for` is inclusive on both ends (`for i = 1, 3` runs for `i` in
`1, 2, 3`). The optional third expression is the step; the default step is
`1`; a step of `0` is a type error.

### Functions and scripts

```
fn_def     ::= 'fn' ident '(' [ ident { ',' ident } ] ')' '{' newline { item } '}'
script_block ::= 'script' '{' raw_lua_text '}'
call_stmt  ::= expr_call                # a call used as a statement
```

A `fn` body is ordinary Scorium items, not Lua. `return` exits the function
and may carry a value. A `script { }` body is raw Lua text handed verbatim to
the sandboxed runtime -- it is not transpiled and is never reformatted.

### Includes

```
include    ::= 'include' string_literal
```

The path is a quoted string or a bare string literal. Resolution, cycle
detection, and traversal policy are runtime behaviour (see EMBEDDING.md).

---

## Expressions

```
expr       ::= or_expr
or_expr    ::= and_expr { 'or' and_expr }
and_expr   ::= cmp_expr { 'and' cmp_expr }
cmp_expr   ::= add_expr [ rel_op add_expr ]
add_expr   ::= mul_expr { add_op mul_expr }
mul_expr   ::= unary { mul_op unary }
unary      ::= unary_op postfix | postfix
postfix    ::= primary { call_suffix | member_suffix }
call_suffix::= '(' [ expr { ',' expr } ] ')'
member_suffix ::= '.' ident
primary    ::= int | float | bool | 'nil' | color | duration
             | quoted_str | bare_str | list | '(' expr ')' | ident
list       ::= '[' [ expr { ',' expr } ] ']'
```

### Strings

```
quoted_str ::= '"' { char | escape } '"'        # literal, no interpolation
bare_str   ::= { bare_text | interp }           # interpolation allowed
escape     ::= '\' ('"' | '\' | 'n' | 't' | 'r')
```

`$name` interpolation is allowed **only** inside a bare string. Inside a
quoted string `$name` is literal text.

### Identifier resolution

A bare `ident` used where an expression is expected resolves at evaluation
time, in this order:

1. a lexical local / loop variable / function parameter;
2. an `@`-defined variable;
3. a sibling leaf emitted earlier in the same node body;
4. a host-registered value;
5. otherwise, the identifier **falls back to a literal string**.

The fallback to a literal string is what lets `select(kitty, alacritty,
foot)` skip quoting. A `$name` in a bare string has no fallback: if it is not
defined, it is an undefined-interpolation error.

### Reassignment

Scorium has no separate assignment statement. A `key = value` leaf whose key
names an existing `local` updates that local instead of emitting a leaf
entry. This is required so a `while` loop can advance its own counter:

```scor
local i = 0
while i < 3 do
    i = i + 1
end
```

`@`-defined variables are **not** reassignable through this mechanism; only
`local`s are.

---

## Reserved words

The following are reserved and cannot be used as a bare node name or
identifier-as-value in the obvious way:

```
if  then  elseif  else  end
for  do  while
fn  local  return  nil  true  false
and  or  not
include  script
```

Using one as a node name produces a `reserved_word` diagnostic.

---

## What is implemented vs. deferred

### Implemented

- All leaf value types in the table in LANGUAGE.md.
- Nodes, nested nodes, node headers.
- `@`-definitions, `$`-interpolation, plain identifier references, sibling
  leaf access.
- Arithmetic, comparison, boolean, unary operators (with the spaced-operator
  rule).
- Function calls, member access on colors (`darken`/`lighten`/`alpha`).
- `if`/`elseif`/`else`, numeric `for` (with optional step), `while`, `local`,
  `return`, `fn`.
- `script { }` raw Lua under the sandbox.
- `include` with cycle detection and path policy.
- All comment forms (`#`, `--`, `--[[ ]]`).

### Deferred (not in this version)

- **Host-pluggable literal *syntax*** -- new lexer-level token shapes
  registered by a host (for example a bespoke `10MB` byte-size token). The
  host-defined *type* extension point exists today (it validates an
  already-parsed `Value`); bespoke lexer syntax does not.
- **Generic `for` over tables/iterators** -- only the numeric `for` is
  supported. `script { }` blocks can still use Lua's full generic `for`
  against `math`/`string`/`table`.
- **Schema file format** -- schemas are built in Rust (see scorium-schema).
  There is deliberately no `.scor`-based schema language yet.
- **String escapes beyond `\" \\ \n \t \r`** (for example `\u{...}`).
- **Block-comment nesting** and comment tracking inside expressions/lists.
