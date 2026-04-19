# Nightjar Language

A declarative, prefix-notation DSL for formal verification of structured data,
delivered as an embeddable Rust library.

Nightjar expressions look like this:

```
(AND (GE .revenue 0) (LT .revenue 1000000))
```

Given a data payload, every expression reduces to exactly one of three outcomes:

- `True`  — the assertion holds,
- `False` — the assertion does not hold,
- `Error` — the assertion could not be evaluated (bad type, missing symbol,
  divide-by-zero, overflow, …).

The three-valued result is central: a well-formed "no" is not the same thing as
an ill-formed expression. Both are reported, both carry a source span, both can
be acted on by the host. Named after the nightjar — a bird that catches bugs —
this crate is the language engine of the broader Nightjar verification framework.

---

## Table of contents

1. [When to reach for it](#when-to-reach-for-it)
2. [A 30-second taste](#a-30-second-taste)
3. [Installation](#installation)
4. [The language at a glance](#the-language-at-a-glance)
5. [Using the library](#using-the-library)
6. [Building clients — worked examples](#building-clients--worked-examples)
7. [Error handling in practice](#error-handling-in-practice)
8. [Safety and performance notes](#safety-and-performance-notes)
9. [Further reading](#further-reading)
10. [License](#license)

---

## When to reach for it

**Good fit.**

- Validating API request / response payloads against declarative rules.
- Config, rule-set, or policy validation where the rules themselves are data
  (stored in a database, an API, a file, or pushed by an admin UI).
- Data-quality assertions in ETL pipelines: "every row satisfies X".
- Anywhere you want to store a validation predicate as a short, auditable text
  string and evaluate it later.

**Not a fit.**

- General-purpose programming. Nightjar has no variables, no loops, no I/O,
  no user-defined functions, no side effects, no module system.
- Anything that must mutate state or call out to other systems.

Nightjar is intentionally small; complexity belongs in the host application.

---

## A 30-second taste

Each of these is a complete, runnable Nightjar expression. They grow in power.

```text
;; 1. Look up a field and compare to a literal.
(GT .revenue 0)

;; 2. Combine assertions with logical connectives.
(AND (GE .revenue 0) (LT .revenue 1000000))

;; 3. Assert a property of every element in a list, using `@` to refer
;;    to the current element. Here: every test result's expected field
;;    equals its actual field.
(ForAll (EQ @.expected @.actual) .results)
```

(Semicolon lines are only for this README. Nightjar has no comment syntax.)

---

## Installation

Nightjar is published as a Rust crate.

```toml
# Cargo.toml

[dependencies]
# Default: `json` feature on — convenience entry points accept serde_json::Value.
nightjar-lang = "0.1"

# Core only — no JSON bridge, smaller dependency tree.
# nightjar-lang = { version = "0.1", default-features = false }

# Opt into YAML support (brings in serde_yaml).
# nightjar-lang = { version = "0.1", features = ["yaml"] }
```

Features:

| Feature | Default | What it brings                                           |
|---------|---------|----------------------------------------------------------|
| `json`  | yes     | `exec(expr, json_value, opts)`, `SymbolTable::from_json`, `From<serde_json::Value> for Entity` |
| `yaml`  | no      | `serde_yaml` dependency (reserved for YAML data bridges) |

Nightjar targets stable Rust, edition 2021, and the standard library. No
`no_std`, no WASM target today.

---

## The language at a glance

Everything is `(Operator arg1 arg2 …)`. Prefix notation eliminates operator
precedence; parentheses are the only grouping mechanism.

### Types

Seven runtime types. No implicit coercion except one targeted exception:
`Int` auto-promotes to `Float` when the other operand of an arithmetic
function or a comparison verifier is a `Float`. Everything else that mixes
types is a `TypeError`.

| Type     | Example literals            | Notes                                         |
|----------|-----------------------------|-----------------------------------------------|
| `Int`    | `42`, `-7`, `0`             | 64-bit signed, checked arithmetic             |
| `Float`  | `3.14`, `-0.5`, `1.0`       | IEEE 754 double                               |
| `String` | `"hello"`, `"營收"`, `""`   | UTF-8, no escape sequences in literals        |
| `Bool`   | `True`, `False`             |                                               |
| `List`   | (built from host data)      | Ordered, heterogeneous                        |
| `Map`    | (built from host data)      | String-keyed, heterogeneous, hash-backed      |
| `Null`   | `Null`                      | Always "empty" for `NonEmpty` purposes        |

### Symbols — how expressions talk to data

There are two symbol namespaces:

- `.` is **root-rooted** — resolved against the whole input payload.
  Examples: `.revenue`, `.data.department_1.revenue`, bare `.` = the whole root.
- `@` is **element-rooted** — resolved against the current iteration element
  of the nearest enclosing `ForAll` / `Exists`. Only legal inside a quantifier
  predicate; using `@` outside one is a `ScopeError` (caught at parse time).
  Examples: `@.a`, `@._1.name`, bare `@` = the whole current element.

Paths are dot-separated. Segments are Unicode-aware — `.營收`, `.données.résultat`
are all valid keys. For lists, elements are addressed with `_0`, `_1`, … (0-based):

```text
(GT .ids._1 15)            ;; second element of .ids > 15
(EQ .matrix._0._0 1)       ;; matrix[0][0] == 1
```

### Operator cheat-sheet

All operators are tabulated below. For full semantics — edge cases, empty
inputs, NaN handling, the exact rules each operator enforces — see
[SUPPLEMENT.md](SUPPLEMENT.md).

**Verifiers** (two values → `Bool`).
`EQ`, `NE`, `LT`, `LE`, `GT`, `GE`.
Example: `(EQ .a .b)`, `(LT .count 100)`.

**Unary check** (one value → `Bool`).
`NonEmpty` — false for `Null`, empty `String`/`List`/`Map`; true otherwise.
Example: `(NonEmpty .name)`, `(NonEmpty .)`.

**Connectives** (booleans → `Bool`).
`AND`, `OR` (binary); `NOT` (unary).
Example: `(AND (NonEmpty .name) (GE .age 18))`, `(NOT (EQ .status "inactive"))`.

**Quantifiers** (predicate + list → `Bool`).
`ForAll`, `Exists`. The predicate may be a *partial verifier* `(GT 0)`, the
bare `NonEmpty`, or a full boolean expression using `@`.
Examples:

```text
(ForAll (GT 0) .scores)                             ;; every score > 0
(Exists NonEmpty .names)                            ;; any name non-empty
(ForAll (EQ (Add @.a @.b) @.c) .rows)               ;; each row: a + b == c
```

**Arithmetic** (numbers → number).

| Op    | Arity | Notes                                              |
|-------|-------|----------------------------------------------------|
| `Add` | 2     | Int+Int stays Int; mixing with Float promotes      |
| `Sub` | 2     |                                                    |
| `Mul` | 2     |                                                    |
| `Div` | 2     | Int/Int truncates; `Div _ 0` is an error           |
| `Mod` | 2     | Works on Int and Float; `Mod _ 0` is an error      |
| `Neg` | 1     |                                                    |
| `Abs` | 1     |                                                    |

Integer arithmetic is checked — overflow produces `IntegerOverflow`, not
wraparound.

**String** (strings → string or int).

| Op          | Arity | Notes                                               |
|-------------|-------|-----------------------------------------------------|
| `Concat`    | 2     | `(Concat "a" "b")` → `"ab"`                         |
| `Length`    | 1     | Unicode scalar count (characters, not bytes)        |
| `Substring` | 3     | `(Substring s start len)` — char-indexed            |
| `Upper`     | 1     | Unicode-aware                                       |
| `Lower`     | 1     | Unicode-aware                                       |

**Collection** (container → value or container).

| Op          | Arity | Notes                                                       |
|-------------|-------|-------------------------------------------------------------|
| `Head`      | 1     | First element of a list; error on empty                     |
| `Tail`      | 1     | All but first; error on empty                               |
| `Get`       | 2     | `(Get list Int)` or `(Get map String)`; out-of-range errors |
| `Count`     | 1     | Size of list or map                                         |
| `GetKeys`   | 1     | Map → sorted list of String keys (deterministic order)      |
| `GetValues` | 1     | Map → list of values, sorted by key (deterministic order)   |

---

## Using the library

The full public API is re-exported from the crate root (`use nightjar_lang::*`).

### The one-line path

With the default `json` feature, a single call does everything:

```rust
use nightjar_lang::{exec, ExecOptions, ExecResult};
use serde_json::json;

fn main() {
    let r = exec(
        "(AND (GE .revenue 0) (LT .revenue 1000000))",
        json!({ "revenue": 42_000 }),
        ExecOptions::default(),
    );
    assert_eq!(r, ExecResult::True);
}
```

### Without the `json` feature

Build an `Entity` from your host data and call `exec_entity`:

```rust
use nightjar_lang::{exec_entity, Entity, ExecOptions, ExecResult};
use std::collections::HashMap;

fn main() {
    let mut root = HashMap::new();
    root.insert("revenue".to_string(), Entity::Int(42_000));
    let data = Entity::Map(root);

    let r = exec_entity(
        "(AND (GE .revenue 0) (LT .revenue 1000000))",
        data,
        ExecOptions::default(),
    );
    assert_eq!(r, ExecResult::True);
}
```

Scalar `From` impls (`From<i64>`, `From<f64>`, `From<bool>`, `From<String>`,
`From<&str>`) are always available. With the `json` feature,
`From<serde_json::Value>` is added.

### Inspecting `ExecResult`

```rust
use nightjar_lang::ExecResult;

fn check(r: ExecResult) {
    if r.is_true() {
        println!("holds");
    } else if r.is_false() {
        println!("does not hold");
    } else if r.is_error() {
        if let ExecResult::Error(e) = &r {
            eprintln!("{:?} at {:?}: {}", e.code(), e.span(), e.message());
        }
    }
}
```

`ExecResult` is `PartialEq`, so tests can assert against `ExecResult::True` /
`ExecResult::False` directly.

### Tuning the executor

```rust
use nightjar_lang::ExecOptions;

let opts = ExecOptions {
    float_epsilon: 1e-9,   // default: 1e-10 — raise to be more lenient on float EQ
    max_depth:     512,    // default: 256  — raise only if you deliberately allow deep rules
};
```

- `float_epsilon` controls `EQ`/`NE` on floats: two floats are equal iff
  `|a - b| < epsilon`. Ordering verifiers (`LT`, `LE`, `GT`, `GE`) use
  standard IEEE 754 comparison and ignore epsilon.
- `max_depth` is the parser's nesting-depth guard. Pathological input
  exceeding it produces a `RecursionError` before any evaluation happens.

### Parse once, evaluate many

`exec`/`exec_entity` re-parse the expression on every call. For hot loops,
parse up-front with `parse` or `parse_with_config` and hold onto the `Program`:

```rust
use nightjar_lang::{parse, Program};

let program: Program = parse("(GE .revenue 0)").expect("rule syntax");
// ... later, re-use `program` against many payloads.
```

The current public `exec` entry points always go through the full parse →
evaluate path; if you need to amortize parsing across many payloads, keep the
`Program` around and wrap your own evaluation loop (the AST is fully public in
`nightjar_lang::{BoolExpr, ValueExpr, …}`). The public evaluator is exposed via
`exec_entity`; a future release may add a `Program`-accepting entry point.

---

## Building clients — worked examples

Three complete, runnable clients demonstrating the intended embedding patterns.
Each assumes the `Cargo.toml` from the [Installation](#installation) section.

### 1. A CLI validator

Reads a Nightjar rule from `argv[1]`, a JSON payload from stdin, prints
`True` / `False` / the error, exits `0` for `True`, `1` for `False`, `2` for
error. Useful for shell pipelines and CI gates.

```toml
# Cargo.toml
[package]
name = "nightjar-verify"
version = "0.1.0"
edition = "2021"

[dependencies]
nightjar-lang = "0.1"
serde_json    = "1"
```

```rust
// src/main.rs
use nightjar_lang::{exec, ExecOptions, ExecResult};
use std::io::{self, Read};

fn main() {
    let mut args = std::env::args().skip(1);
    let rule = match args.next() {
        Some(r) => r,
        None => {
            eprintln!("usage: nightjar-verify '<rule>' < payload.json");
            std::process::exit(64); // EX_USAGE
        }
    };

    let mut raw = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut raw) {
        eprintln!("read stdin: {}", e);
        std::process::exit(74); // EX_IOERR
    }
    let data: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("invalid JSON on stdin: {}", e);
            std::process::exit(65); // EX_DATAERR
        }
    };

    match exec(&rule, data, ExecOptions::default()) {
        ExecResult::True  => { println!("True");  std::process::exit(0); }
        ExecResult::False => { println!("False"); std::process::exit(1); }
        ExecResult::Error(e) => {
            eprintln!("{:?} at {:?}: {}", e.code(), e.span(), e.message());
            std::process::exit(2);
        }
    }
}
```

Shell usage:

```sh
echo '{"revenue": 4200}' \
  | nightjar-verify '(AND (GE .revenue 0) (LT .revenue 1000000))'
# True   (exit 0)
```

### 2. A web service

An Axum handler that accepts `POST /validate` with body
`{"rule": "...", "data": {...}}` and responds with `200 True`, `400 False`,
or `400 Error + diagnostic`. Drop-in pattern for any Rust web framework;
the `ExecResult` → HTTP mapping is the part that matters.

```toml
# Cargo.toml
[package]
name = "nightjar-verify-service"
version = "0.1.0"
edition = "2021"

[dependencies]
nightjar-lang = "0.1"
axum          = "0.7"
serde         = { version = "1", features = ["derive"] }
serde_json    = "1"
tokio         = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
// src/main.rs
use axum::{extract::Json, http::StatusCode, routing::post, Router};
use nightjar_lang::{exec, ExecOptions, ExecResult};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct VerifyRequest {
    rule: String,
    data: serde_json::Value,
}

async fn verify(Json(req): Json<VerifyRequest>)
    -> (StatusCode, Json<serde_json::Value>)
{
    match exec(&req.rule, req.data, ExecOptions::default()) {
        ExecResult::True  => (StatusCode::OK,          Json(json!({"result": "True"}))),
        ExecResult::False => (StatusCode::BAD_REQUEST, Json(json!({"result": "False"}))),
        ExecResult::Error(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "result":  "Error",
                "code":    format!("{:?}", e.code()),
                "span":    { "start": e.span().start, "end": e.span().end },
                "message": e.message(),
            })),
        ),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/validate", post(verify));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Example call:

```sh
curl -sS http://127.0.0.1:3000/validate \
  -H 'content-type: application/json' \
  -d '{"rule":"(GE .revenue 0)","data":{"revenue":42}}'
# {"result":"True"}
```

### 3. A batch checker over a JSONL file

Streams `payloads.jsonl`, evaluates each line against a single rule, and
reports the offending records with their error diagnostics.

```toml
# Cargo.toml
[package]
name = "nightjar-batch"
version = "0.1.0"
edition = "2021"

[dependencies]
nightjar-lang = "0.1"
serde_json    = "1"
```

```rust
// src/main.rs
use nightjar_lang::{exec, ExecOptions, ExecResult};
use std::fs::File;
use std::io::{BufRead, BufReader};

fn main() -> std::io::Result<()> {
    let rule = "(AND (GE .revenue 0) (LT .revenue 1000000))";
    let path = std::env::args().nth(1).expect("usage: nightjar-batch payloads.jsonl");
    let reader = BufReader::new(File::open(path)?);

    let mut failures  = 0usize;
    let mut error_hit = 0usize;

    for (lineno, line) in reader.lines().enumerate() {
        let line = line?;
        let data: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("line {}: bad JSON: {}", lineno + 1, e);
                error_hit += 1;
                continue;
            }
        };
        match exec(rule, data, ExecOptions::default()) {
            ExecResult::True  => {}
            ExecResult::False => {
                failures += 1;
                println!("line {}: False", lineno + 1);
            }
            ExecResult::Error(e) => {
                error_hit += 1;
                println!(
                    "line {}: Error {:?} at {:?}: {}",
                    lineno + 1, e.code(), e.span(), e.message()
                );
            }
        }
    }

    eprintln!("done: {} failures, {} errors", failures, error_hit);
    Ok(())
}
```

---

## Error handling in practice

Every error carries three stable pieces of information:

```rust
use nightjar_lang::{exec, ExecOptions, ExecResult};

let r = exec("(GT .absent 0)", serde_json::json!({}), ExecOptions::default());
if let ExecResult::Error(e) = r {
    println!("code    = {:?}", e.code());     // E004
    println!("span    = {:?}", e.span());     // Span { start: ..., end: ... }
    println!("message = {}",   e.message());  // symbol `.absent` not found
}
```

The ten error codes:

| Code | Variant            | Typical trigger                                                  |
|------|--------------------|------------------------------------------------------------------|
| E001 | `ParseError`       | Expression does not conform to the grammar.                      |
| E002 | `TypeError`        | Operator applied to incompatible types (e.g. `(GT "a" 1)`).      |
| E003 | `ArgumentError`    | Wrong operand count (e.g. `(GT 1 2 3)`).                         |
| E004 | `SymbolNotFound`   | Symbol path not present in the payload.                          |
| E005 | `AmbiguousSymbol`  | Reserved for a future shorthand-lookup mode; not raised today.   |
| E006 | `DivisionByZero`   | `Div`/`Mod` with a zero divisor (Int or Float).                  |
| E007 | `RecursionError`   | AST nesting exceeds `max_depth`.                                 |
| E008 | `IndexError`       | `Get`/`Head`/`Tail` run off the end of a list.                   |
| E009 | `IntegerOverflow`  | Checked integer arithmetic overflowed.                           |
| E010 | `ScopeError`       | `@` symbol used outside any quantifier predicate.                |

For the exact triggering conditions and minimal reproducers of each code,
see [SUPPLEMENT.md](SUPPLEMENT.md).

---

## Safety and performance notes

- **Depth limit.** The parser rejects any expression nested deeper than
  `ExecOptions::max_depth` (default 256), so adversarial input cannot blow
  the host's stack.
- **Symbol-table construction is O(N).** The table is a flattened
  `HashMap<String, Entity>` built once per evaluation: every intermediate
  map/list path is registered. Lookup during evaluation is then O(1) per
  root-rooted symbol.
- **List size is the host's responsibility.** Very large lists grow the
  flattened table linearly (one entry per element, plus nested paths).
  Trim, sample, or page large collections upstream before injecting them.
- **Determinism.** `Map` uses `HashMap` internally, but `GetKeys` /
  `GetValues` sort by key, so operators that iterate maps produce the same
  output every run.
- **Checked arithmetic.** Integer ops never wrap silently; float ops never
  overflow to zero — overflow surfaces as `IntegerOverflow` (E009) and bad
  float input surfaces via ordinary IEEE 754 (NaN comparisons return `false`).

---

## Further reading

- [SUPPLEMENT.md](SUPPLEMENT.md) — full EBNF grammar, complete operator
  semantics (including every edge case), architecture reference, full error
  table, and a guide to extending the language (adding new functions,
  operators, or types).

---

## License

Licensed under the Apache License, Version 2.0.
See [LICENSE](LICENSE) for details.

Copyright © Wayne Hong (h-alice) &lt;contact@halice.art&gt;.
