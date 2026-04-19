# Nightjar Language — Supplement

Comprehensive reference for contributors, maintainers, and AI coding agents.
This document is a superset of [README.md](README.md): it reproduces the formal
grammar, defines every operator's exact semantics (including edge cases),
describes the crate's internal architecture, catalogs every error code, and
provides step-by-step recipes for extending the language.

A new user of the library does **not** need to read this document to use
Nightjar. A contributor designing a new operator, a new quantifier, a new data
type, or a new parser subsystem **does**.

---

## Table of contents

1. [Design philosophy](#design-philosophy)
2. [Formal language specification](#formal-language-specification)
3. [Operator semantics](#operator-semantics)
4. [Symbol table and flattening](#symbol-table-and-flattening)
5. [Execution pipeline](#execution-pipeline)
6. [Public API reference](#public-api-reference)
7. [Error codes — full reference](#error-codes--full-reference)
8. [Architecture and module layout](#architecture-and-module-layout)
9. [Extending the language](#extending-the-language)
10. [Testing strategy](#testing-strategy)
11. [Design decisions and rationale](#design-decisions-and-rationale)
12. [Known limitations and deferred features](#known-limitations-and-deferred-features)
13. [License](#license)

---

## Design philosophy

Nightjar is built around five non-negotiable principles. Every design decision
below follows from one of them; when a change proposes to break one, treat it
as a substantial change that requires explicit justification.

1. **Correctness first.** Every syntactically valid expression reduces to a
   well-defined result in `{ True, False, Error }`. There is no "undefined",
   no "maybe", no silent fallback to a default value. Errors are first-class,
   carry spans and codes, and are distinguishable from a well-formed `False`.
2. **Minimal surface.** The language is intentionally tiny. It has no
   variables, no lambdas, no user-defined functions, no I/O, no modules,
   no side effects, no state. Complexity belongs in the host application,
   not in the DSL.
3. **Formal foundations.** Every operator has a precise mathematical
   definition. Quantifiers mean what they mean in first-order logic;
   verifiers are binary relations; connectives are the Boolean algebra.
   The authoritative EBNF lives in-tree ([src/language/grammar.rs](src/language/grammar.rs))
   and this document reproduces it verbatim.
4. **Strictness.** No implicit type coercion, with one carefully bounded
   exception: `Int` promotes to `Float` when the other operand of an
   arithmetic function or comparison verifier is a `Float`. Every other
   cross-type combination is a `TypeError`. Missing symbols are
   `SymbolNotFound`; they do not default to `Null`.
5. **Safety.** Nightjar is always embedded in someone else's application.
   It must not crash the host on adversarial input. The parser enforces a
   configurable depth limit; integer arithmetic is checked; list unrolling
   is O(N) and the host is responsible for bounding the input.

Unicode is not a separate principle — it is a direct consequence of
correctness. Identifiers, string content, and map keys all go through
`char::is_alphanumeric`, which recognises Unicode letter and number
categories (L* and N*), so `.營收`, `.données.résultat`, and
`"紅嘴黑鵯"` are all first-class.

---

## Formal language specification

### EBNF grammar

This block is reproduced verbatim from
[src/language/grammar.rs:23-110](src/language/grammar.rs#L23-L110), which is the
authoritative copy. When editing the grammar, update both places in the same
commit; see the EBNF drift check in [Extending the language](#extending-the-language).

```ebnf
(* A program is a single expression that must reduce to Boolean. *)
program         = bool_expr ;

bool_expr       = bool_literal
                | verifier_expr
                | connective_expr
                | not_expr
                | unary_check_expr
                | quantifier_expr ;

verifier_expr   = "(" verifier_op value_expr value_expr ")" ;
verifier_op     = "EQ" | "NE" | "LT" | "LE" | "GT" | "GE" ;

connective_expr = "(" connective_op bool_expr bool_expr ")" ;
connective_op   = "AND" | "OR" ;
not_expr        = "(" "NOT" bool_expr ")" ;

unary_check_expr = "(" "NonEmpty" value_expr ")" ;

(* Quantifiers: assert a predicate over a List-typed Entity. *)
quantifier_expr = "(" quantifier_op predicate value_expr ")" ;
quantifier_op   = "ForAll" | "Exists" ;

(* Predicates are only legal inside quantifiers.                  *)
(* Partial vs. full predicate is disambiguated by operand count   *)
(* at parse time, not by a syntactic marker:                      *)
(*   (VerifierOp x)       — partial verifier (1 operand)          *)
(*   (VerifierOp x y)     — full bool_expr (2 operands)           *)
(*   NonEmpty (bare)      — unary check                           *)
(*   any other bool_expr  — full bool_expr                        *)
(* The body of a full predicate may use the "@" element-rooted    *)
(* symbol form to refer to the current iteration element.         *)
predicate       = partial_verifier | "NonEmpty" | bool_expr ;
partial_verifier = "(" verifier_op value_expr ")" ;

(* Value expressions produce an Entity. *)
value_expr      = literal
                | symbol
                | func_expr ;

(* Arity is enforced at parse time from FuncOp::expected_arity():  *)
(*   1-ary: Neg, Abs, Length, Upper, Lower,                        *)
(*          Head, Tail, Count, GetKeys, GetValues                  *)
(*   2-ary: Add, Sub, Mul, Div, Mod, Concat, Get                   *)
(*   3-ary: Substring                                              *)
func_expr       = "(" func_op value_expr { value_expr } ")" ;
func_op         = arith_op | string_op | collection_op ;
arith_op        = "Add" | "Sub" | "Mul" | "Div"
                | "Mod" | "Neg" | "Abs" ;
string_op       = "Concat" | "Length" | "Substring"
                | "Upper" | "Lower" ;
collection_op   = "Head" | "Tail" | "Get" | "Count"
                | "GetKeys" | "GetValues" ;

(* Terminals. *)
literal         = int_literal | float_literal
                | string_literal | bool_literal | null_literal ;

(* A leading "-" is part of the numeric literal    *)
int_literal     = [ "-" ] digit { digit } ;
float_literal   = [ "-" ] digit { digit } "." digit { digit } ;

string_literal  = '"' { any_char } '"' ;
bool_literal    = "True" | "False" ;
null_literal    = "Null" ;

(* Symbols have two namespaces:                                    *)
(*   "." — root-rooted (resolved against the whole input).         *)
(*   "@" — element-rooted (resolved against the current iteration  *)
(*         element of the nearest enclosing ForAll/Exists).        *)
(* Bare "." is the whole input; bare "@" is the current element.   *)
(* "@" is only legal inside a quantifier predicate; the parser     *)
(* rejects it elsewhere with a ParseError.                         *)
symbol          = ( "." | "@" ) [ segment { "." segment } ] ;

(* Segment characters are Unicode-aware:                           *)
(* char::is_alphanumeric() covers Unicode categories L* and N*,    *)
(* so keys like ".營收" and ".données.résultat" are valid.         *)
segment         = ident_start { ident_char } ;
ident_start     = unicode_letter | "_" ;
ident_char      = unicode_letter | unicode_digit | "_" ;

digit           = "0" | "1" | "2" | "3" | "4"
                | "5" | "6" | "7" | "8" | "9" ;
```

### Lexical rules

- **Whitespace-insensitive.** Spaces, tabs, and newlines only separate tokens.
  `(GT 1 2)`, `(GT  1   2)`, and `( GT\n  1\n  2\n)` are identical.
- **No comments.** Nightjar expressions have no comment syntax. Comments and
  documentation live in the host program, not in the rule text.
- **Numeric literals.** A `-` immediately followed by a digit is part of the
  number: `-5` and `-3.14` are single tokens. `- 5` (with a space) is a
  `ParseError` because `-` is not a standalone token.
- **String literals.** Double-quoted. There are no escape sequences defined;
  every character between the opening and closing quotes is literal, including
  any whitespace and any Unicode scalar. An unterminated string (missing
  closing quote before EOF) is a `ParseError`.
- **Keywords vs identifiers.** Operator names (`EQ`, `Add`, `ForAll`, …),
  boolean literals (`True`, `False`), and `Null` are keywords. They are case-
  sensitive — `true` is not a literal, `add` is not an operator. Keywords
  cannot be used as symbol segments because they do not start with `.` or `@`.
- **Symbol segments.** A segment starts with `ident_start` (Unicode letter or
  `_`) and continues with `ident_char` (Unicode letter or digit or `_`). This
  means `.1x` is a `ParseError` but `._1` is a valid list-index segment.
  Segments are joined by literal `.` characters.
- **Symbol sigils.** `.` starts a root-rooted symbol; `@` starts an element-
  rooted symbol. Bare `.` and bare `@` (with no segments) refer to the whole
  payload and the whole current element respectively.

### Data types

The seven runtime types are defined by the `Entity` enum in
[src/context/entity.rs:53-61](src/context/entity.rs#L53-L61):

| Entity variant     | Underlying Rust type       | Literal in the language | "Empty" for `NonEmpty` |
|--------------------|----------------------------|-------------------------|------------------------|
| `Entity::Int`      | `i64`                      | `42`, `-7`              | never empty            |
| `Entity::Float`    | `f64`                      | `3.14`, `-0.5`          | never empty            |
| `Entity::String`   | `String`                   | `"hello"`, `"營收"`     | empty iff `""`         |
| `Entity::Bool`     | `bool`                     | `True`, `False`         | never empty            |
| `Entity::List`     | `Vec<Entity>`              | (from host data)        | empty iff `[]`         |
| `Entity::Map`      | `HashMap<String, Entity>`  | (from host data)        | empty iff `{}`         |
| `Entity::Null`     | (unit)                     | `Null`                  | always empty           |

`Entity::type_tag()` projects to a `TypeTag` enum, used throughout the
runtime for type checks and error messages.

### Type coercion

The only implicit coercion in the language is **Int → Float auto-promotion**,
and it applies in exactly two places:

- **Arithmetic functions** `Add`, `Sub`, `Mul`, `Div`, `Mod`: if either
  operand is `Float`, the other (if `Int`) is promoted, and the result is
  `Float`. Both `Int` → `Int` arithmetic. Both `Float` → `Float` arithmetic.
- **Comparison verifiers** `EQ`, `NE`, `LT`, `LE`, `GT`, `GE`: when one side
  is `Int` and the other is `Float`, the `Int` is promoted before comparison.

Every other type mismatch is a `TypeError` (E002). `(Add 1 "abc")`,
`(GT "a" 1)`, `(Concat 1 2)`, `(Head 42)` are all errors.

`Null` is never silently converted. A `Null` operand to an arithmetic op is
a `TypeError`, a `Null` operand to `NonEmpty` is always `False`, and
`SymbolNotFound` is the rule for missing keys (not `Null`).

---

## Operator semantics

Every operator below is listed with arity, input types, output type, and
every edge case worth documenting. Operators are grouped by family, matching
the AST enums in [src/language/grammar.rs](src/language/grammar.rs).

### Verifiers — `EQ NE LT LE GT GE`

Binary, two value expressions → `Bool`. Implemented in
[src/context/verifier.rs](src/context/verifier.rs).

- **Equality (`EQ`, `NE`) on `Float`** uses **epsilon-based comparison**:
  `EQ(a, b) ⇔ |a − b| < ε`, where `ε = ExecOptions::float_epsilon`
  (default `1e-10`). This is what makes `(EQ (Add 0.1 0.2) 0.3)` evaluate to
  `True`, despite IEEE 754 representation error. `NE` is the negation.
- **Ordering verifiers (`LT`, `LE`, `GT`, `GE`) on `Float`** use standard
  IEEE 754 comparison (`partial_cmp`). Epsilon does not apply.
- **NaN** — any comparison involving NaN (EQ, NE, LT, LE, GT, GE) returns
  `false`. This matches Rust's `partial_cmp` semantics and IEEE 754.
  Specifically, `(EQ NaN NaN)` is `False` (because
  `|NaN − NaN|` is `NaN`, not `< ε`).
- **Int ↔ Float promotion** applies for mixed-type compares.
- **String equality** is exact byte equality (which is also Unicode scalar
  equality for canonicalised UTF-8 strings).
- **Bool equality** is the obvious thing.
- **Cross-type comparisons** (e.g. `(GT "a" 1)`, `(EQ .list .int)`) are a
  `TypeError`.
- **Null equality** — `(EQ Null Null)` is `True`. `(EQ Null anything_else)`
  is a `TypeError`: we deliberately do not let `Null` silently equal scalars.

### Unary check — `NonEmpty`

Unary, one value → `Bool`. Returns the result of `Entity::is_non_empty()`
([src/context/entity.rs:81-89](src/context/entity.rs#L81-L89)):

| Input          | `NonEmpty` result |
|----------------|-------------------|
| `Int`, `Float`, `Bool` (any value) | `True`   |
| `String ""`                          | `False`  |
| `String "anything else"`             | `True`   |
| `List []`                            | `False`  |
| `List [ … ]`                         | `True`   |
| `Map {}`                             | `False`  |
| `Map { … }`                          | `True`   |
| `Null`                               | `False`  |

### Connectives — `AND OR NOT`

`AND` and `OR` are binary boolean-in, boolean-out; `NOT` is unary.
Implemented in [src/context/connective.rs](src/context/connective.rs).

- **No short-circuit evaluation.** Both operands of `AND`/`OR` are always
  evaluated. If one branch produces an `Error`, the error surfaces immediately
  (regardless of whether the other branch would have decided the result).
  This keeps error behaviour deterministic — every error in a rule is
  surfaced, never masked by a short-circuit.
- **Adding short-circuit later is compatible** with the API shape, but would
  change the observable error behaviour. If it is ever added, it must be
  opt-in (e.g. via `ExecOptions`) so existing rules keep their diagnostic
  behaviour.

### Quantifiers — `ForAll Exists`

`(QuantifierOp predicate operand)`. Implemented in
[src/context/quantifier.rs](src/context/quantifier.rs).

- **Predicate forms.** Three shapes are accepted, disambiguated at parse time
  by operand count:
  - `NonEmpty` (bare) — unary check, applied to the element.
  - `(VerifierOp x)` — *partial verifier*: the bound value `x` is the second
    operand of the verifier; the element fills the first. So
    `(ForAll (GT 0) xs)` means "∀e ∈ xs. e > 0".
  - Any other `bool_expr` — *full predicate*: re-evaluated once per element
    with the element bound as `@` in scope. The body can use `@`, `@.field`,
    `@._i`, etc.
- **Operand must be a `List`.** Passing a `Map` is a `TypeError` — quantifiers
  iterate over ordered sequences. For Maps, convert explicitly with
  `GetKeys` or `GetValues`: `(ForAll (GT 0) (GetValues .m))`.
- **Scalar fallback.** If the operand is a scalar (`Int`, `Float`, `String`,
  `Bool`, `Null`), the quantifier reduces to a single predicate application
  on that scalar. So `(ForAll (GT 0) 5)` is `True`, `(Exists (EQ 2) 10)` is
  `False`. This is intentional and documented — it lets callers treat "one
  value" and "many values" uniformly.
- **Empty list.** `(ForAll p [])` is `True` (vacuously true);
  `(Exists p [])` is `False` (no witness exists).
- **Nested quantifiers.** `@` always refers to the **innermost** enclosing
  element. Outer elements are accessible only through root-rooted paths
  (e.g. `.outer.inner.field`). This is lexical, innermost-wins scoping.
- **`@` outside a quantifier predicate** is a `ScopeError` (E010), caught by
  `validate_scope` during post-parse static analysis (see
  [Execution pipeline](#execution-pipeline)).
- **Evaluation strategy.** Partial verifiers and `NonEmpty` use
  `apply_quantifier`, which resolves the bound operand once and applies the
  predicate per element. Full predicates use `apply_quantifier_full`, which
  takes a closure that invokes `eval_bool` per element with the element
  bound in the `scope` parameter. Full predicates therefore re-evaluate
  their body N times for N elements.

### Arithmetic — `Add Sub Mul Div Mod Neg Abs`

Implemented in [src/context/function.rs](src/context/function.rs).

- **Input types.** `Int` or `Float`. Anything else is `TypeError`.
- **Int + Int → Int** using `checked_add`, `checked_sub`, `checked_mul`,
  `checked_div`, `checked_rem`, `checked_neg`. Overflow → `IntegerOverflow`
  (E009). In particular `Abs(i64::MIN)` and `Neg(i64::MIN)` are overflow.
- **Mixed Int/Float** → Int is promoted, result is `Float`.
- **Float + Float → Float** using native IEEE operations. No overflow error;
  inputs that would overflow return `inf`/`-inf`, and NaN arithmetic
  propagates in the usual IEEE way.
- **Integer division truncates.** `(Div 7 2)` is `Int(3)`. For real division,
  promote explicitly: `(Div 7 2.0)` is `Float(3.5)`.
- **Division/modulo by zero** — both `Int 0` and `Float 0.0` divisors produce
  `DivisionByZero` (E006). Nightjar does not produce `inf` or NaN from
  `1.0 / 0.0`; we raise an error for consistency with integer semantics.
- **`Mod` works on floats.** `(Mod 3.5 1.5)` is `Float(0.5)` via Rust's `%`.
- **`Neg`, `Abs`** are unary; every other arithmetic op is binary.

### String — `Concat Length Substring Upper Lower`

- **`Concat`** (2-ary, `String × String → String`).
- **`Length`** (1-ary, `String → Int`). **Counts Unicode scalar values**, not
  bytes. `(Length "abc")` is `3`; `(Length "營收")` is `2`. This is what
  `Substring` indexes into — the two are consistent.
- **`Substring`** (3-ary, `String × Int × Int → String`). `(Substring s start
  len)` returns `len` characters starting at character index `start` (0-based,
  char-indexed). Going off the end of the string is an error; see
  [src/context/function.rs](src/context/function.rs) for the exact bounds.
- **`Upper`, `Lower`** (1-ary) — Unicode-aware case folding via Rust's
  `to_uppercase`/`to_lowercase`. Characters without a case variant pass
  through unchanged.
- Any non-String argument is a `TypeError`.

### Collection — `Head Tail Get Count GetKeys GetValues`

- **`Head`** (1-ary) — first element of a list. Empty list → `IndexError`
  (E008). Non-list input → `TypeError`.
- **`Tail`** (1-ary) — list of all but the first element. Empty list →
  `IndexError`. Non-list input → `TypeError`.
- **`Get`** (2-ary) — polymorphic index:
  - `(Get list Int)` returns the element at that 0-based index. Out of range
    → `IndexError`. Negative indices are not supported.
  - `(Get map String)` returns the value at that key. Missing key →
    `SymbolNotFound` with a message scoped to `Get`.
  - Any other combination is a `TypeError`.
- **`Count`** (1-ary) — length of a `List` or size of a `Map`. Non-container
  input is a `TypeError`.
- **`GetKeys`** (1-ary) — `Map → List<String>`, sorted by key for
  determinism. Non-map input is a `TypeError`.
- **`GetValues`** (1-ary) — `Map → List<Entity>`, values sorted by key
  (same ordering as `GetKeys`). Non-map input is a `TypeError`.

---

## Symbol table and flattening

Root-rooted (`.`) symbols are resolved against a **flattened symbol table**
built once per evaluation. The construction is in
[src/symbol_table.rs](src/symbol_table.rs).

### Flattening rules

Starting from the root `Entity`, every nested path is registered with its
fully qualified dotted key:

- The root itself is registered under `"."`.
- Each `Map` child is registered under `{parent}.{key}`.
- Each `List` element is registered under `{parent}._{i}` with `i` the
  **0-based** index.
- Recursion continues into nested maps and lists.
- Scalars and `Null` are registered at their current prefix; they are not
  descended into.

### Worked example

```json
{
  "ids":  [10, 20, 30],
  "meta": {"name": "x"}
}
```

Flattens to (all entries live in the same `HashMap<String, Entity>`):

| Key             | Value                   |
|-----------------|-------------------------|
| `.`             | the whole root `Map`    |
| `.ids`          | `List [10, 20, 30]`     |
| `.ids._0`       | `Int 10`                |
| `.ids._1`       | `Int 20`                |
| `.ids._2`       | `Int 30`                |
| `.meta`         | `Map { name: "x" }`     |
| `.meta.name`    | `String "x"`            |

Nested containers chain naturally: `{m: [[1,2],[3,4]]}` produces `.m._0._0 =
1`, `.m._1._1 = 4`, etc.

### Resolution

- **Root-rooted (`.path`).** `HashMap::get` — O(1) amortised. Missing path
  → `SymbolNotFound`.
- **Element-rooted (`@path`).** Resolved by `resolve_in_entity` in
  [src/symbol_table.rs](src/symbol_table.rs): walks the `path` directly
  against the current element `Entity`. No flattening involved — cost is
  O(path length), and there's no extra allocation of a per-element table.
  `_N` segments are still list-index segments with the same 0-based convention.

### Invariants to preserve

Anything that touches the symbol table must preserve these invariants, or
quantifiers and lookups will silently disagree:

1. The flattening convention (`.` for maps, `._N` for lists, 0-based) must
   match `resolve_in_entity`'s walking convention.
2. Intermediate containers must be registered (not only leaves), so
   `(NonEmpty .data)` works on the container as a whole.
3. `HashMap` is allowed to iterate in arbitrary order internally, but any
   operator that exposes ordering to the user (today: `GetKeys`, `GetValues`)
   must sort.

---

## Execution pipeline

Nightjar is strictly two-phase. The entry points in
[src/executor.rs](src/executor.rs) drive both phases, but they are cleanly
separable — `parse` / `parse_with_config` give you Phase 1 alone.

```
  source string  ──►  tokens  ──►  AST (Spanned<…>)  ──►  ExecResult
                │              │                     │
                │              │                     └── Phase 2: symbol table + scope
                │              └── Phase 1b: parser + validate_scope
                └── Phase 1a: tokenizer
```

### Phase 1a — Tokenizer

Located in [src/language/parser.rs](src/language/parser.rs). Walks the source
with `char_indices` so all byte offsets land on character boundaries
(UTF-8-safe). Produces `Spanned<Token>` values. Highlights:

- **Negative literals.** `-5` and `-3.14` are single tokens when the `-` is
  immediately followed by a digit. `- 5` (with a space between) is a
  `ParseError` because `-` is not a standalone token.
- **Strings.** No escape sequences. An unterminated string literal
  (`"abc` with EOF before the closing quote) is a `ParseError` with a span
  pointing at the opening quote.
- **Keywords.** Case-sensitive. The tokenizer has an explicit keyword table
  for operator names and reserved literals.
- **Symbols.** `.` and `@` sigils with dot-separated segments. Segment
  characters are validated against `char::is_alphanumeric` (Unicode L* and
  N* categories) plus `_`.

### Phase 1b — Parser

Recursive-descent over the token stream. Key properties:

- Per-operator arity is enforced at parse time using `FuncOp::expected_arity`
  ([src/language/grammar.rs](src/language/grammar.rs)), so `(Add 1)` and
  `(Substring "a" 0)` are caught before any evaluation.
- Depth tracking uses `ParserConfig::max_depth` (default 256). Exceeding it
  produces `RecursionError` (E007). The default is tunable via
  `ExecOptions::max_depth` → `ParserConfig::max_depth`.
- Every AST node is wrapped in `Spanned<T>` carrying the span of the
  originating tokens, so runtime errors can point back into the source
  string.

### Phase 1c — Scope validator

`validate_scope` ([src/language/parser.rs](src/language/parser.rs)) is a
post-parse AST walk that tracks an integer *predicate depth* counter.

- Entering the predicate position of a `Quantifier` increments the counter.
- Leaving it decrements.
- The quantifier's *operand* position stays at the current depth.
- Encountering an `@` symbol with counter `== 0` raises `ScopeError` (E010).

This catches `(EQ @.a 1)` at the top level, or `(AND (ForAll … .xs) (EQ @.a 1))`
where the second `@` is outside any predicate.

### Phase 2 — Executor

[src/executor.rs](src/executor.rs) drives evaluation through two mutually
recursive functions:

- `eval_bool(expr, symbols, opts, scope)` — evaluates a `SpannedBoolExpr` to
  `Result<bool, NightjarLanguageError>`. Dispatches on the `BoolExpr` variant.
- `eval_value(expr, symbols, opts, scope)` — evaluates a `SpannedValueExpr` to
  `Result<Entity, …>`. Dispatches on `ValueExpr`.

The `scope` parameter is `Option<&Entity>` — the current iteration element
bound inside a quantifier predicate, or `None` at the top level. Element-
rooted (`@`) symbol resolution reads from `scope`; a `None` `scope` combined
with an `@` symbol is a defensive `ScopeError` (in practice `validate_scope`
catches this first).

The quantifier arm branches on predicate kind:

- **Partial verifier / `NonEmpty`** → `resolve_predicate` pre-evaluates the
  bound operand once, then calls
  `quantifier::apply_quantifier(op, &EvalPredicate, &operand, epsilon, span)`.
- **Full predicate** → calls
  `quantifier::apply_quantifier_full(op, &operand, span, closure)` where
  `closure: &Entity → Result<bool, …>` invokes `eval_bool` with the element
  bound in `scope`. Full predicates re-evaluate their body per element, which
  is how `@` inside the body resolves.

Top-level evaluation always starts with `scope = None`.

---

## Public API reference

All of the following are re-exported from the crate root
([src/lib.rs](src/lib.rs)). Consumers should `use nightjar_lang::{…}`.

### Parser

```rust
pub fn parse(input: &str) -> Result<Program, NightjarLanguageError>;

pub fn parse_with_config(
    input: &str,
    config: &ParserConfig,
) -> Result<Program, NightjarLanguageError>;

pub struct ParserConfig {
    pub max_depth: usize,   // default 256
}
```

`parse` is a convenience wrapper around `parse_with_config` using the default
`ParserConfig`. Both return a `Program` whose top-level expression is a
`SpannedBoolExpr`.

### AST

```rust
pub struct Program { pub expr: SpannedBoolExpr; }

pub struct Spanned<T> { pub node: T, pub span: Span; }
pub type   SpannedBoolExpr  = Spanned<BoolExpr>;
pub type   SpannedValueExpr = Spanned<ValueExpr>;

pub enum BoolExpr {
    Literal(bool),
    Verifier    { op: VerifierOp,    left:  Box<SpannedValueExpr>,
                                     right: Box<SpannedValueExpr> },
    And(Box<SpannedBoolExpr>, Box<SpannedBoolExpr>),
    Or (Box<SpannedBoolExpr>, Box<SpannedBoolExpr>),
    Not(Box<SpannedBoolExpr>),
    UnaryCheck  { op: UnaryCheckOp,  operand: Box<SpannedValueExpr> },
    Quantifier  { op: QuantifierOp,
                  predicate: Spanned<Predicate>,
                  operand:   Box<SpannedValueExpr> },
}

pub enum ValueExpr {
    Literal(Literal),
    Symbol   { root: SymbolRoot, path: String },
    FuncCall { op: FuncOp, args: Vec<SpannedValueExpr> },
}

pub enum Predicate {
    PartialVerifier { op: VerifierOp, bound: Box<SpannedValueExpr> },
    UnaryCheck(UnaryCheckOp),
    Full(Box<SpannedBoolExpr>),
}

pub enum Literal  { Int(i64), Float(f64), String(String), Bool(bool), Null }

pub enum VerifierOp   { EQ, NE, LT, LE, GT, GE }
pub enum UnaryCheckOp { NonEmpty }
pub enum QuantifierOp { ForAll, Exists }
pub enum FuncOp {
    Add, Sub, Mul, Div, Mod, Neg, Abs,
    Concat, Length, Substring, Upper, Lower,
    Head, Tail, Get, Count, GetKeys, GetValues,
}
pub enum Keyword { /* unified keyword enum used by the tokenizer */ }
```

`Spanned<T>` exists so every AST node carries its source span for diagnostics;
future passes that want to annotate nodes should wrap in `Spanned` rather
than threading spans separately.

### Runtime

```rust
pub enum Entity {
    Int(i64), Float(f64), String(String), Bool(bool),
    List(Vec<Entity>), Map(std::collections::HashMap<String, Entity>), Null,
}

pub enum TypeTag { Int, Float, String, Bool, List, Map, Null }

impl Entity {
    pub fn type_tag(&self) -> TypeTag;
    pub fn is_non_empty(&self) -> bool;
}

// Always-on conversions:
impl From<i64>     for Entity;
impl From<f64>     for Entity;
impl From<bool>    for Entity;
impl From<String>  for Entity;
impl From<&str>    for Entity;

// With the `json` feature:
#[cfg(feature = "json")]
impl From<serde_json::Value> for Entity;
```

```rust
pub struct SymbolTable { /* private */ }

impl SymbolTable {
    pub fn from_entity(root: Entity) -> Self;
    pub fn resolve(&self, symbol: &str, span: Span)
        -> Result<Entity, NightjarLanguageError>;
    pub fn resolve_root_path(&self, path: &str, span: Span)
        -> Result<Entity, NightjarLanguageError>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn contains(&self, symbol: &str) -> bool;
}

#[cfg(feature = "json")]
impl SymbolTable {
    pub fn from_json(value: serde_json::Value) -> Self;
}
```

```rust
pub struct ExecOptions {
    pub float_epsilon: f64,   // default 1e-10
    pub max_depth:     usize, // default 256
}
impl Default for ExecOptions { /* the defaults above */ }

pub enum ExecResult { True, False, Error(NightjarLanguageError) }

impl ExecResult {
    pub fn is_true(&self) -> bool;
    pub fn is_false(&self) -> bool;
    pub fn is_error(&self) -> bool;
}

impl From<Result<bool, NightjarLanguageError>> for ExecResult;

pub fn exec_entity(expression: &str, data: Entity, options: ExecOptions)
    -> ExecResult;

#[cfg(feature = "json")]
pub fn exec(expression: &str, data: serde_json::Value, options: ExecOptions)
    -> ExecResult;
```

### Errors

```rust
pub struct Span { pub start: usize, pub end: usize }
impl Span {
    pub const fn new(start: usize, end: usize) -> Self;
    pub const fn point(at: usize)               -> Self;
}

pub enum ErrorCode { E001, E002, E003, E004, E005, E006, E007, E008, E009, E010 }

pub enum NightjarLanguageError {
    ParseError       { span: Span, code: ErrorCode, message: String },
    TypeError        { span: Span, code: ErrorCode, message: String },
    ArgumentError    { span: Span, code: ErrorCode, message: String },
    SymbolNotFound   { span: Span, code: ErrorCode, message: String },
    AmbiguousSymbol  { span: Span, code: ErrorCode, message: String },
    DivisionByZero   { span: Span, code: ErrorCode, message: String },
    RecursionError   { span: Span, code: ErrorCode, message: String },
    IndexError       { span: Span, code: ErrorCode, message: String },
    IntegerOverflow  { span: Span, code: ErrorCode, message: String },
    ScopeError       { span: Span, code: ErrorCode, message: String },
}

impl NightjarLanguageError {
    pub fn span(&self)    -> Span;
    pub fn code(&self)    -> ErrorCode;
    pub fn message(&self) -> &str;
}
```

Error construction helpers (`parse_error`, `type_error`, …) live in
[src/error.rs](src/error.rs) and are `pub(crate)` — they are internal
conveniences, not part of the public API. Downstream code inspects errors
through `.code()`, `.span()`, `.message()`.

---

## Error codes — full reference

Every variant of `ErrorCode` that the implementation can actually raise,
with minimal reproducing expressions or conditions.

| Code | Variant            | Raised by              | Minimal reproducer                                                 |
|------|--------------------|------------------------|--------------------------------------------------------------------|
| E001 | `ParseError`       | Tokenizer, parser      | `GT 1 2` (no parens); `(GT 1 2` (unclosed); `"abc` (unterminated). |
| E002 | `TypeError`        | Verifier, functions, quantifier | `(GT "a" 1)`; `(Head 42)`; `(ForAll (GT 0) .map)`.        |
| E003 | `ArgumentError`    | Parser (arity check)   | `(GT 1 2 3)`; `(Add 1)`; `(Substring "a" 0)`.                      |
| E004 | `SymbolNotFound`   | Symbol resolver, `Get` on Map | `(GT .absent 0)` against `{}`; `(Get .m "missing")`.       |
| E005 | `AmbiguousSymbol`  | Reserved — not raised today | *(no reproducer; placeholder for future shorthand lookup)*     |
| E006 | `DivisionByZero`   | `Div`, `Mod`           | `(Div 1 0)`; `(Mod 1 0.0)`.                                        |
| E007 | `RecursionError`   | Parser (depth guard)   | `(NOT (NOT (NOT …)))` deeper than `max_depth` (default 256).       |
| E008 | `IndexError`       | `Head`, `Tail`, `Get` on List | `(Head [])`; `(Tail [])`; `(Get [1,2] 5)`.                  |
| E009 | `IntegerOverflow`  | Checked arithmetic     | `(EQ (Add 9223372036854775807 1) 0)`.                              |
| E010 | `ScopeError`       | `validate_scope` (and defensive runtime check) | `(EQ @.a 1)` at top level.                 |

E005 is reserved for a future shorthand-lookup mode (leaf-name resolution
with ambiguity detection). Tools should accept it as a valid code but should
not expect to see it from the current executor.

---

## Architecture and module layout

Everything lives under `src/`.

| Path | Responsibility |
|------|----------------|
| [src/lib.rs](src/lib.rs) | Crate root and public re-exports. The authoritative list of what is `pub`. |
| [src/error.rs](src/error.rs) | `NightjarLanguageError`, `ErrorCode`, `Span`, internal `pub(crate)` helper constructors. |
| [src/language/grammar.rs](src/language/grammar.rs) | AST types, operator enums (`VerifierOp`, `FuncOp`, `QuantifierOp`, `UnaryCheckOp`, `Keyword`), `Predicate`, `Literal`, `SymbolRoot`, `Spanned`, `FuncOp::expected_arity`, authoritative EBNF in the module doc-comment. |
| [src/language/parser.rs](src/language/parser.rs) | Tokenizer, recursive-descent parser, `ParserConfig`, `parse`, `parse_with_config`, post-parse `validate_scope`. |
| [src/symbol_table.rs](src/symbol_table.rs) | `SymbolTable`, flattening algorithm, `resolve_in_entity` (element-rooted walker). |
| [src/executor.rs](src/executor.rs) | `ExecOptions`, `ExecResult`, `exec`, `exec_entity`, private `eval_bool` / `eval_value` / `resolve_predicate`. |
| [src/context/mod.rs](src/context/mod.rs) | Module grouping. |
| [src/context/entity.rs](src/context/entity.rs) | `Entity`, `TypeTag`, `is_non_empty`, `From` impls (including `serde_json::Value` under the `json` feature). |
| [src/context/verifier.rs](src/context/verifier.rs) | `apply_verifier` — EQ/NE/LT/LE/GT/GE dispatch, epsilon equality, NaN handling. |
| [src/context/function.rs](src/context/function.rs) | `apply_function` — arithmetic, string, collection functions. |
| [src/context/quantifier.rs](src/context/quantifier.rs) | `EvalPredicate`, `apply_predicate`, `apply_quantifier`, `apply_quantifier_full`. |
| [src/context/connective.rs](src/context/connective.rs) | `apply_and`, `apply_or`, `apply_not`. |
| [tests/test_parser.rs](tests/test_parser.rs) | Phase-1 integration tests. |
| [tests/test_executor.rs](tests/test_executor.rs) | Phase-2 integration tests. |

The directory structure mirrors the two-phase pipeline: `language/*` is
everything the parser needs, `context/*` is everything the runtime needs,
and `executor.rs` + `symbol_table.rs` glue them together.

---

## Extending the language

All recipes below assume you are editing the crate in-place. Every extension
should ship with tests — see [Testing strategy](#testing-strategy).

### Recipe A — Add a new built-in function

Suppose you are adding a `Reverse` function that takes a `String` or a `List`
and returns the reversed value.

1. **Grammar layer** — [src/language/grammar.rs](src/language/grammar.rs):
   - Add `Reverse` to `FuncOp`.
   - Add an entry in `FuncOp::expected_arity` returning `1`.
   - Add a keyword constant for `"Reverse"` to the `Keyword` enum (and any
     operator-name → `Keyword` mapping used by the tokenizer).
   - Update the EBNF comment to list `Reverse` under `arith_op` /
     `string_op` / `collection_op` as appropriate. Keep this block in sync
     with this document's `## Formal language specification` section.
2. **Tokenizer** — [src/language/parser.rs](src/language/parser.rs):
   - Register the keyword string so the tokenizer emits the new `Keyword`
     variant.
3. **Parser** — [src/language/parser.rs](src/language/parser.rs):
   - `func_expr` parsing is driven by `FuncOp::expected_arity`, so usually
     nothing new is needed. Verify by adding a parse test.
4. **Runtime** — [src/context/function.rs](src/context/function.rs):
   - Extend the match arms in `apply_function` to handle `FuncOp::Reverse`.
   - Return the right `TypeTag`-tagged result; use `type_error` for bad
     input types; reuse the existing error helpers.
5. **Public re-exports** — [src/lib.rs](src/lib.rs):
   - No change is needed if `FuncOp` is already re-exported (it is).
6. **Tests**:
   - Add unit tests in `#[cfg(test)] mod tests` inside
     [src/context/function.rs](src/context/function.rs) for the happy path
     and each error branch.
   - Add at least one integration test in
     [tests/test_parser.rs](tests/test_parser.rs) (parses) and
     [tests/test_executor.rs](tests/test_executor.rs) (evaluates).
7. **Documentation**:
   - Update the operator table in [README.md](README.md) under *Operator
     cheat-sheet*.
   - Update the relevant subsection under *Operator semantics* in this file.

### Recipe B — Add a new verifier

Adding, say, `Contains` (string contains substring):

1. Add `Contains` to `VerifierOp` (or, if it's genuinely a new family,
   create a new enum alongside `VerifierOp`). If in doubt, prefer a new
   family — verifiers are currently defined as total orders plus equality,
   and `Contains` breaks that.
2. If it lands in `VerifierOp`: extend `apply_verifier` in
   [src/context/verifier.rs](src/context/verifier.rs) with the new arm,
   including type checks and `TypeError` for bad inputs.
3. Extend tokenizer, parser arity, and EBNF as in Recipe A.
4. Tests + docs as in Recipe A.

### Recipe C — Add a new quantifier

Example: `Count` (count elements satisfying a predicate) — note this would
return an `Int`, not a `Bool`, so it belongs in a new family (value-producing
quantifier), not in `QuantifierOp`.

1. Decide whether it is boolean-returning (goes alongside `ForAll`/`Exists`)
   or value-returning (goes alongside `FuncOp`). Boolean quantifiers reuse
   the `Quantifier` arm of `BoolExpr`; value-returning quantifiers need a
   new AST variant — plan that change first.
2. For a boolean quantifier: add a variant to `QuantifierOp`; extend
   `apply_quantifier` / `apply_quantifier_full` with the new reduction;
   extend `eval_bool`'s quantifier arm if new predicate shapes are needed.
3. For a value-returning quantifier: add a new `ValueExpr` variant (e.g.
   `ValueQuantifier { op, predicate, operand }`), extend the parser with a
   new parse arm, add an executor arm in `eval_value`. Re-export the new
   AST types from `lib.rs`.
4. Scope validator: entering the predicate position must still increment
   `predicate_depth`, otherwise `@` will escape.
5. Tests + docs as in Recipe A.

### Recipe D — Add a new data type

Any change to `Entity` is load-bearing; every operator that inspects
`TypeTag` potentially needs updating.

1. Add the variant to `Entity` and `TypeTag` in
   [src/context/entity.rs](src/context/entity.rs). Implement `type_tag()`
   and `is_non_empty()` — both must remain total.
2. Provide `From` impls as appropriate for host integrations. If the `json`
   feature has to represent the new type, update `From<serde_json::Value>`.
3. Update the flattener in [src/symbol_table.rs](src/symbol_table.rs) so
   that the new type flattens correctly (either descend or not, but make
   the choice explicitly).
4. Update `apply_verifier` in
   [src/context/verifier.rs](src/context/verifier.rs) — decide equality
   semantics for the new type, and whether ordering makes sense. Cross-type
   comparisons must remain `TypeError`.
5. Update `apply_function` in
   [src/context/function.rs](src/context/function.rs) — every existing op
   must either accept or reject the new type explicitly (current match arms
   must gain a `_ => TypeError` path if they don't already).
6. Update `apply_quantifier` scalar fallback path to decide whether the new
   type supports iteration or scalar fallback.
7. Update `resolve_in_entity` in
   [src/symbol_table.rs](src/symbol_table.rs): if the new type is
   path-addressable (like Map/List) add a walker arm; otherwise let the
   `_ => TypeError` branch catch it.
8. Tests + docs; update the type table in both [README.md](README.md) and
   the *Data types* subsection here.

### Recipe E — Swap the Map backing or the `Clone` strategy

If you replace `HashMap<String, Entity>` with a different container, the
only externally-visible invariant that must survive is that `GetKeys` and
`GetValues` produce sorted output. If you replace `Entity: Clone` with
`Rc<Entity>`-sharing, every `From` impl, every `apply_*` signature, and
every executor arm that clones will need touching — plan the change as a
whole crate refactor, not an incremental one, and keep the public API
stable.

### EBNF drift check

The EBNF in this file must match the EBNF block in
[src/language/grammar.rs:23-110](src/language/grammar.rs#L23-L110) exactly.
When you add an operator, update both and diff them in your commit. If they
drift, the parser and the documentation disagree and the next contributor
will act on the wrong one.

---

## Testing strategy

Nightjar has three layers of tests.

1. **Module-local unit tests.** Every non-trivial module has
   `#[cfg(test)] mod tests { … }` right at the bottom. These are the first
   line of defence for new behaviour. Every helper function and every match
   arm should have at least one happy-path test and one error-branch test
   (where an error branch exists).
2. **Integration tests.** [tests/test_parser.rs](tests/test_parser.rs) and
   [tests/test_executor.rs](tests/test_executor.rs) exercise the public API
   end-to-end: `parse`, `exec`, `exec_entity`, `ExecResult`, error variants.
   When you add an operator, add at least one parser test (it parses) and
   one executor test (it evaluates correctly on real data).
3. **Property-based testing.** `proptest` is in `[dev-dependencies]`. For
   operators with algebraic properties (associativity of `Concat`,
   commutativity of `Add` on `Int`, idempotence of `Upper ∘ Upper`, …),
   property tests are the appropriate form. Prefer them to hand-rolled
   edge-case tables for anything fuzz-adjacent.

### Running tests

```sh
cargo test                           # default features (json on)
cargo test --no-default-features     # core-only build (no serde_json)
cargo test --features yaml           # yaml dep compiled in
```

CI should run all three to prevent feature-gated regressions.

---

## Design decisions and rationale

### Why prefix notation?

Prefix (S-expression-style) notation removes operator precedence and
associativity entirely. There is no "does `AND` bind tighter than `OR`?"
question because every expression is fully parenthesised. The parser is a
few hundred lines, the grammar is small enough to fit in this document,
and the AST shape is exactly the expression's surface shape. An infix
surface syntax could be added externally later as a layer that compiles
to this AST — the canonical form stays prefix.

### Why a three-valued `ExecResult`?

Formal verification loses its meaning if a missing key silently becomes
`Null` and the rule silently becomes `False`. The host cannot tell a
rule-was-false from rule-could-not-be-evaluated. By carving `Error` out
from the result type, Nightjar forces the host to decide how to handle
each case (log, fail-open, fail-closed, retry, …) rather than collapsing
them at the library boundary.

### Why epsilon equality on floats but IEEE ordering?

Equality is the comparison most sensitive to IEEE 754 representation
error: `0.1 + 0.2 != 0.3` is a foot-gun that Nightjar rules should not
step on. Ordering is much less sensitive to the same error (the relative
ordering of two floats is preserved even when their binary representations
drift a ulp), and the IEEE rules for ordering are already what users expect
from comparisons. Mixing the two would require users to reason about an
epsilon in contexts where it doesn't help them.

### Why 0-based list indexing via `_N`?

0-based aligns with Rust, JavaScript, Python, C, and nearly every modern
language; 1-based would surprise most implementers. The `_` prefix keeps
the index segment syntactically distinct from map keys (which start with a
letter or digit-less identifier), and the same convention is used both in
the flat symbol table and in `resolve_in_entity`.

### Why flatten into a HashMap?

Most Nightjar rules look up several fields of the same payload; a flat
table makes each lookup O(1) after a single O(N) build. Path-walking at
every symbol reference would be cheaper in memory but much more expensive
per lookup, especially for rules with many references. The trade-off
matters most for wide, shallow data (typical API payloads); it's worse for
very long lists, which is why the host is expected to bound list size.

### Why no short-circuit in AND / OR today?

Error visibility. If `AND` short-circuits and the right-hand side would
have errored, the rule's author never learns. Non-short-circuit evaluation
surfaces every error, which is the behaviour a verification tool wants.
If a future release adds opt-in short-circuit (via `ExecOptions`), it must
document that errors in the skipped branch are hidden.

### Why `@` as a separate sigil, not a lambda?

A lambda would bring first-class functions, closures over names, and a
name-resolution layer into the language. Nightjar is deliberately first-
order — predicates are syntactic forms, not values. `@` is a lexical
marker that means "the current element of the innermost quantifier". It
has no runtime representation other than a value binding, and it cannot
escape its quantifier.

---

## Known limitations and deferred features

- **Shorthand symbol resolution (E005).** Looking up a leaf name like
  `revenue` without the full path `.data.revenue` and reporting
  `AmbiguousSymbol` when it matches multiple paths is planned but not
  implemented. The strict, fully-qualified form is the only form today.
- **Short-circuit evaluation.** Not available today; see the rationale
  above.
- **REPL.** There is no interactive shell for Nightjar rules; the batch/
  CLI pattern in the README serves the same purpose.
- **Infix → prefix converter.** An external convenience tool would let
  users write `1 + 2 > 0` and compile it to `(GT (Add 1 2) 0)`. Out of
  scope for the language itself; a reasonable standalone crate.
- **Currying beyond quantifier predicates.** The partial-verifier form
  `(GT 0)` is the only currying the language does. Generalising it is
  possible (arity-based disambiguation already discriminates partial from
  full) but explicitly deferred.
- **`no_std` / WASM.** Not a current target. Neither `std` removal nor a
  dedicated WASM build is in scope today.
- **Unbounded list unrolling.** The flattener registers one symbol-table
  entry per list element. The host is responsible for bounding list
  sizes before passing data to Nightjar; there is no configurable
  upper bound in the library.
- **`Program`-accepting `exec`.** Today, `exec` / `exec_entity` re-parse
  on every call. A future release may add a variant that accepts a
  pre-built `Program` for hot loops; for now, consumers that need
  parse-once behaviour can drive evaluation themselves using the public
  AST.

---

## License

Licensed under the Apache License, Version 2.0.
See [LICENSE](LICENSE) for the full text.

Copyright © Wayne Hong (h-alice) &lt;contact@halice.art&gt;.
