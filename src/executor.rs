// Copyright 2026 Wayne Hong (h-alice) <contact@halice.art>
// Nightjar Language Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Core executor.
//!
//! Parses a Nightjar language expression and evaluates the resulting AST
//! against a flattened symbol table, yielding a three-valued ExecResult
//! (True / False / Error).

use crate::context::entity::Entity;
use crate::context::{connective, function, quantifier, verifier};
use crate::error::{scope_error, NightjarLanguageError};
use crate::language::grammar::{
    BoolExpr, Literal, Predicate, Program, SpannedBoolExpr, SpannedValueExpr, SymbolRoot,
    UnaryCheckOp, ValueExpr,
};
use crate::language::parser::{parse_with_config, ParserConfig};
use crate::symbol_table::{resolve_in_entity, SymbolTable};

/// Execution options, configurable per invocation.
#[derive(Debug, Clone)]
pub struct ExecOptions {
    /// Tolerance for epsilon-based `EQ`/`NE` on floats.
    pub float_epsilon: f64,
    /// Max nesting depth enforced by the parser.
    pub max_depth: usize,
}

impl Default for ExecOptions {
    fn default() -> Self {
        Self {
            float_epsilon: 1e-10,
            max_depth: 256,
        }
    }
}

/// Three-valued execution result.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecResult {
    /// The assertion holds.
    True,
    /// The assertion does not hold.
    False,
    /// The assertion could not be evaluated; the wrapped
    /// [`NightjarLanguageError`] carries the diagnostic.
    Error(NightjarLanguageError),
}

impl ExecResult {
    /// Return `true` when the result is [`ExecResult::True`].
    pub fn is_true(&self) -> bool {
        matches!(self, ExecResult::True)
    }

    /// Return `true` when the result is [`ExecResult::False`].
    pub fn is_false(&self) -> bool {
        matches!(self, ExecResult::False)
    }

    /// Return `true` when the result is [`ExecResult::Error`].
    pub fn is_error(&self) -> bool {
        matches!(self, ExecResult::Error(_))
    }
}

impl From<Result<bool, NightjarLanguageError>> for ExecResult {
    fn from(r: Result<bool, NightjarLanguageError>) -> Self {
        match r {
            Ok(true) => ExecResult::True,
            Ok(false) => ExecResult::False,
            Err(e) => ExecResult::Error(e),
        }
    }
}

/// Main entry point against an already-built Entity.
pub fn exec_entity(expression: &str, data: Entity, options: ExecOptions) -> ExecResult {
    let cfg = ParserConfig {
        max_depth: options.max_depth,
    };
    let program = match parse_with_config(expression, &cfg) {
        Ok(p) => p,
        Err(e) => return ExecResult::Error(e),
    };
    let symbols = SymbolTable::from_entity(data);
    eval_program(&program, &symbols, &options).into()
}

/// Convenience entry point to ingest JSON directly.
#[cfg(feature = "json")]
pub fn exec(expression: &str, data: serde_json::Value, options: ExecOptions) -> ExecResult {
    exec_entity(expression, Entity::from(data), options)
}

/// Drive evaluation of the top-level `Program`.
///
/// Since our program is a boolean expression, we use this thin wrapper that
/// delegates to [`eval_bool`] with an initially empty scope (no element binding
/// only the root symbol table is visible).
///
/// Example:
///
/// ```ignore
/// use crate::executor::{eval_program, ExecOptions};
/// use crate::language::parser::parse;
/// use crate::symbol_table::SymbolTable;
/// use crate::context::entity::Entity;
///
/// let program  = parse("(EQ 1 1)").unwrap();
/// let symbols  = SymbolTable::from_entity(Entity::Null);
/// let opts     = ExecOptions::default();
/// assert!(eval_program(&program, &symbols, &opts).unwrap());
/// ```
fn eval_program(
    p: &Program,
    symbols: &SymbolTable,
    opts: &ExecOptions,
) -> Result<bool, NightjarLanguageError> {
    eval_bool(&p.expr, symbols, opts, None)
}

/// Recursive evaluator for boolean-producing AST nodes.
///
/// Evaluates on the `BoolExpr` variant, forwards `symbols` for root-rooted
/// lookups, and forwards `scope` (the current iteration element, if any), so
/// that element-relative `@` symbols inside a quantifier predicate resolve
/// against the element rather than the root.
///
/// The `Quantifier` arm branches on the predicate kind: `PartialVerifier` /
/// `UnaryCheck` reuse the pre-resolution path via [`resolve_predicate`] &
/// [`quantifier::apply_quantifier`], while `Predicate::Full(body)` takes the
/// delayed-evaluation path that invokes `eval_bool` per element with the
/// element bound in `scope`.
///
/// Example (internal):
///
/// ```ignore
/// // Evaluate `(GT .x 0)` against `{ x: 5 }`.
/// use crate::executor::{eval_bool, ExecOptions};
/// use crate::language::parser::parse;
/// use crate::symbol_table::SymbolTable;
/// use crate::context::entity::Entity;
/// use std::collections::HashMap;
///
/// let mut m = HashMap::new();
/// m.insert("x".to_string(), Entity::Int(5));
/// let st   = SymbolTable::from_entity(Entity::Map(m));
/// let prog = parse("(GT .x 0)").unwrap();
/// assert!(eval_bool(&prog.expr, &st, &ExecOptions::default(), None).unwrap());
/// ```
fn eval_bool(
    expr: &SpannedBoolExpr,
    symbols: &SymbolTable,
    opts: &ExecOptions,
    scope: Option<&Entity>,
) -> Result<bool, NightjarLanguageError> {
    match &expr.node {
        BoolExpr::Literal(b) => Ok(*b),
        BoolExpr::Verifier { op, left, right } => {
            let l = eval_value(left, symbols, opts, scope)?;
            let r = eval_value(right, symbols, opts, scope)?;
            verifier::apply_verifier(*op, &l, &r, opts.float_epsilon, expr.span)
        }
        BoolExpr::And(l, r) => {
            let lv = eval_bool(l, symbols, opts, scope)?;
            let rv = eval_bool(r, symbols, opts, scope)?;
            Ok(connective::apply_and(lv, rv))
        }
        BoolExpr::Or(l, r) => {
            let lv = eval_bool(l, symbols, opts, scope)?;
            let rv = eval_bool(r, symbols, opts, scope)?;
            Ok(connective::apply_or(lv, rv))
        }
        BoolExpr::Not(inner) => {
            let v = eval_bool(inner, symbols, opts, scope)?;
            Ok(connective::apply_not(v))
        }
        BoolExpr::UnaryCheck { op, operand } => {
            let v = eval_value(operand, symbols, opts, scope)?;
            match op {
                UnaryCheckOp::NonEmpty => Ok(v.is_non_empty()),
            }
        }
        BoolExpr::Quantifier {
            op,
            predicate,
            operand,
        } => {
            // Here's the only place we will actually use `scope`.
            //
            // The operand (the list being iterated) resolves in the current
            // scope, so keep `scope` here and do not shadow with an element yet.
            let coll = eval_value(operand, symbols, opts, scope)?; // Eval into collection in current scope.
            match &predicate.node {
                Predicate::Full(body) => {
                    // For full predicates, we evaluate the body once per element.
                    quantifier::apply_quantifier_full(*op, &coll, expr.span, |element| {
                        eval_bool(body, symbols, opts, Some(element))
                    })
                }
                _ => {
                    // For partial predicates, we resolve the predicate once in the current scope.
                    // So there's no need to evaluate the operand every time.
                    let eval_pred = resolve_predicate(&predicate.node, symbols, opts, scope)?;
                    quantifier::apply_quantifier(
                        *op,
                        &eval_pred,
                        &coll,
                        opts.float_epsilon,
                        expr.span,
                    )
                }
            }
        }
    }
}

/// Recursive evaluator for value-producing AST nodes.
///
/// Converts literals, resolves symbol references (root-rooted against
/// `symbols`, element-rooted against `scope`), and reduces function calls by
/// evaluating every argument and dispatching to [`function::apply_function`].
///
/// Invariants worth remembering:
/// - `ValueExpr::Symbol { root: Element, .. }` with `scope == None` is a
///   runtime `ScopeError` — the static validator normally catches this at
///   parse time, but the check here is a defensive fallback.
/// - `scope` is preserved through recursive calls unchanged; the only
///   function that swaps it is the quantifier dispatch in [`eval_bool`].
///
/// Example (internal):
///
/// ```ignore
/// // Resolve `.x` against a root map.
/// use crate::executor::{eval_value, ExecOptions};
/// use crate::language::grammar::{Spanned, SymbolRoot, ValueExpr};
/// use crate::symbol_table::SymbolTable;
/// use crate::context::entity::Entity;
/// use crate::error::Span;
/// use std::collections::HashMap;
///
/// let mut m = HashMap::new();
/// m.insert("x".to_string(), Entity::Int(42));
/// let st = SymbolTable::from_entity(Entity::Map(m));
/// let expr = Spanned::new(
///     ValueExpr::Symbol { root: SymbolRoot::Root, path: "x".into() },
///     Span::new(0, 2),
/// );
/// let v = eval_value(&expr, &st, &ExecOptions::default(), None).unwrap();
/// assert_eq!(v, Entity::Int(42));
/// ```
#[allow(clippy::only_used_in_recursion)]
fn eval_value(
    expr: &SpannedValueExpr,
    symbols: &SymbolTable,
    opts: &ExecOptions,
    scope: Option<&Entity>,
) -> Result<Entity, NightjarLanguageError> {
    match &expr.node {
        ValueExpr::Literal(lit) => Ok(literal_to_entity(lit)),
        ValueExpr::Symbol { root, path } => match root {
            SymbolRoot::Root => symbols.resolve_root_path(path, expr.span),
            SymbolRoot::Element => match scope {
                Some(elem) => resolve_in_entity(path, elem, expr.span),
                None => Err(scope_error(
                    expr.span,
                    "`@` element-relative symbol evaluated without an enclosing quantifier",
                )),
            },
        },
        ValueExpr::FuncCall { op, args } => {
            let mut evaluated = Vec::with_capacity(args.len());
            for arg in args {
                evaluated.push(eval_value(arg, symbols, opts, scope)?);
            }
            function::apply_function(*op, evaluated, expr.span)
        }
    }
}

/// Pre-resolve the `PartialVerifier` and `UnaryCheck` into an [`EvalPredicate`]
///
/// [`EvalPredicate`] bound operand is already reduced to an `Entity`. This happens
/// once, before the quantifier loop, so scalar bounds like `(GT 0)` aren't
/// re-evaluated per element.
///
/// `Predicate::Full` is intentionally unreachable here, since full predicates
/// need per-element scope binding and are handled directly inside
/// [`eval_bool`]'s `Quantifier` arm.
///
/// Example:
///
/// ```ignore
/// use crate::executor::{resolve_predicate, ExecOptions};
/// use crate::language::grammar::{Predicate, Spanned, ValueExpr, Literal, VerifierOp};
/// use crate::context::quantifier::EvalPredicate;
/// use crate::context::entity::Entity;
/// use crate::symbol_table::SymbolTable;
/// use crate::error::Span;
///
/// // Resolve `(GT 0)`, the bound operand `0` is reduced to Entity::Int(0).
/// let pred = Predicate::PartialVerifier {
///     op: VerifierOp::GT,
///     bound: Box::new(Spanned::new(ValueExpr::Literal(Literal::Int(0)), Span::new(0, 1))),
/// };
/// let st = SymbolTable::from_entity(Entity::Null);
/// let out = resolve_predicate(&pred, &st, &ExecOptions::default(), None).unwrap();
/// assert!(matches!(out, EvalPredicate::PartialVerifier { bound: Entity::Int(0), .. }));
/// ```
fn resolve_predicate(
    pred: &Predicate,
    symbols: &SymbolTable,
    opts: &ExecOptions,
    scope: Option<&Entity>,
) -> Result<quantifier::EvalPredicate, NightjarLanguageError> {
    match pred {
        Predicate::PartialVerifier { op, bound } => {
            let bound_val = eval_value(bound, symbols, opts, scope)?;
            Ok(quantifier::EvalPredicate::PartialVerifier {
                op: *op,
                bound: bound_val,
            })
        }
        Predicate::UnaryCheck(check_op) => Ok(quantifier::EvalPredicate::UnaryCheck(*check_op)),
        // `Full` is handled directly in `eval_bool`'s Quantifier arm because
        // it requires per-element scope binding that `EvalPredicate` can't
        // represent.
        Predicate::Full(_) => unreachable!("Predicate::Full handled in eval_bool"),
    }
}

/// Map an AST `Literal` to its runtime `Entity` counterpart.
///
/// This is a trivial, one-to-one conversion that exists so the evaluator
/// doesn't need to repeat the match arms inline.
///
/// Example (internal):
///
/// ```ignore
/// use crate::executor::literal_to_entity;
/// use crate::language::grammar::Literal;
/// use crate::context::entity::Entity;
///
/// assert_eq!(literal_to_entity(&Literal::Int(7)),       Entity::Int(7));
/// assert_eq!(literal_to_entity(&Literal::Bool(true)),   Entity::Bool(true));
/// assert_eq!(literal_to_entity(&Literal::Null),         Entity::Null);
/// ```
fn literal_to_entity(lit: &Literal) -> Entity {
    match lit {
        Literal::Int(i) => Entity::Int(*i),
        Literal::Float(f) => Entity::Float(*f),
        Literal::String(s) => Entity::String(s.clone()),
        Literal::Bool(b) => Entity::Bool(*b),
        Literal::Null => Entity::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NightjarLanguageError;
    use std::collections::HashMap;

    fn run(expr: &str, data: Entity) -> ExecResult {
        exec_entity(expr, data, ExecOptions::default())
    }

    fn empty() -> Entity {
        Entity::Map(HashMap::new())
    }

    // ── Boolean literals ─────────────────────────────────────

    #[test]
    fn top_level_true_and_false() {
        assert_eq!(run("True", empty()), ExecResult::True);
        assert_eq!(run("False", empty()), ExecResult::False);
    }

    // ── Basic verifiers ──────────────────────────────────────

    #[test]
    fn gt_simple() {
        assert_eq!(run("(GT 1 2)", empty()), ExecResult::False);
        assert_eq!(run("(GT 3 2)", empty()), ExecResult::True);
    }

    #[test]
    fn eq_simple() {
        assert_eq!(run("(EQ 1 1)", empty()), ExecResult::True);
        assert_eq!(run("(EQ 1 2)", empty()), ExecResult::False);
    }

    #[test]
    fn type_error_becomes_exec_error() {
        let r = run("(GT GT 1)", empty());
        // `GT GT 1` is actually a parse error (GT isn't a value expression).
        // Verify the result is an error variant regardless of which.
        assert!(matches!(r, ExecResult::Error(_)));
    }

    // ── Symbol resolution ────────────────────────────────────

    fn map_of(pairs: &[(&str, Entity)]) -> Entity {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        Entity::Map(m)
    }

    #[test]
    fn symbol_verifier() {
        let data = map_of(&[("revenue", Entity::Int(100))]);
        assert_eq!(run("(GE .revenue 100)", data), ExecResult::True);
    }

    #[test]
    fn computed_verification_via_symbols() {
        let data = map_of(&[
            ("dept1", Entity::Int(100)),
            ("dept2", Entity::Int(200)),
            ("total", Entity::Int(300)),
        ]);
        assert_eq!(
            run("(EQ (Add .dept1 .dept2) .total)", data),
            ExecResult::True
        );
    }

    #[test]
    fn connective_and_nonempty() {
        let data = map_of(&[
            ("revenue", Entity::Int(50)),
            ("name", Entity::String("Acme".into())),
        ]);
        assert_eq!(
            run("(AND (GE .revenue 0) (NonEmpty .name))", data),
            ExecResult::True
        );
    }

    // ── Quantifiers ──────────────────────────────────────────

    #[test]
    fn forall_list_positive() {
        let data = map_of(&[(
            "scores",
            Entity::List(vec![Entity::Int(1), Entity::Int(2), Entity::Int(3)]),
        )]);
        assert_eq!(run("(ForAll (GT 0) .scores)", data), ExecResult::True);
    }

    #[test]
    fn forall_list_zero_fails() {
        let data = map_of(&[(
            "scores",
            Entity::List(vec![Entity::Int(0), Entity::Int(1), Entity::Int(2)]),
        )]);
        assert_eq!(run("(ForAll (GT 0) .scores)", data), ExecResult::False);
    }

    #[test]
    fn exists_admin_role() {
        let data = map_of(&[(
            "roles",
            Entity::List(vec![
                Entity::String("user".into()),
                Entity::String("admin".into()),
            ]),
        )]);
        assert_eq!(
            run("(Exists (EQ \"admin\") .roles)", data),
            ExecResult::True
        );
    }

    #[test]
    fn forall_scalar_fallback() {
        let data = map_of(&[("count", Entity::Int(5))]);
        assert_eq!(run("(ForAll (GT 0) .count)", data), ExecResult::True);
    }

    #[test]
    fn forall_map_operand_is_type_error() {
        let data = map_of(&[("data", map_of(&[("a", Entity::Int(1))]))]);
        let r = run("(ForAll (GT 0) .data)", data);
        assert!(matches!(
            r,
            ExecResult::Error(NightjarLanguageError::TypeError { .. })
        ));
    }

    #[test]
    fn forall_over_map_values_via_getvalues() {
        let data = map_of(&[(
            "revenue_by_dept",
            map_of(&[("a", Entity::Int(10)), ("b", Entity::Int(20))]),
        )]);
        assert_eq!(
            run("(ForAll (GE 0) (GetValues .revenue_by_dept))", data),
            ExecResult::True
        );
    }

    // ── Errors propagate ─────────────────────────────────────

    #[test]
    fn missing_symbol_is_exec_error() {
        let r = run("(GT .missing 0)", empty());
        assert!(matches!(
            r,
            ExecResult::Error(NightjarLanguageError::SymbolNotFound { .. })
        ));
    }

    #[test]
    fn division_by_zero_is_exec_error() {
        let r = run("(EQ (Div 1 0) 0)", empty());
        assert!(matches!(
            r,
            ExecResult::Error(NightjarLanguageError::DivisionByZero { .. })
        ));
    }

    #[test]
    fn integer_overflow_is_exec_error() {
        // i64::MAX = 9223372036854775807
        let r = run("(EQ (Add 9223372036854775807 1) 0)", empty());
        assert!(matches!(
            r,
            ExecResult::Error(NightjarLanguageError::IntegerOverflow { .. })
        ));
    }

    // ── Misc ─────────────────────────────────────────────────

    #[test]
    fn nested_arithmetic_evaluates_inside_out() {
        assert_eq!(
            run("(EQ (Add (Mul 2 3) (Sub 10 4)) 12)", empty()),
            ExecResult::True
        );
    }

    #[test]
    fn chained_quantifier_and_count() {
        let data = map_of(&[(
            "scores",
            Entity::List(vec![
                Entity::Int(1),
                Entity::Int(2),
                Entity::Int(3),
                Entity::Int(4),
            ]),
        )]);
        assert_eq!(
            run("(AND (ForAll (GT 0) .scores) (GT (Count .scores) 3))", data),
            ExecResult::True
        );
    }

    #[test]
    fn epsilon_equality_via_default_options() {
        assert_eq!(run("(EQ (Add 0.1 0.2) 0.3)", empty()), ExecResult::True);
    }

    // ── Element-relative symbols (`@`) inside quantifier predicates ──

    fn obj(pairs: &[(&str, Entity)]) -> Entity {
        map_of(pairs)
    }

    #[test]
    fn forall_equal_fields_all_true() {
        let data = map_of(&[(
            "items",
            Entity::List(vec![
                obj(&[("a", Entity::Int(1)), ("b", Entity::Int(1))]),
                obj(&[("a", Entity::Int(2)), ("b", Entity::Int(2))]),
                obj(&[("a", Entity::Int(3)), ("b", Entity::Int(3))]),
            ]),
        )]);
        assert_eq!(run("(ForAll (EQ @.a @.b) .items)", data), ExecResult::True);
    }

    #[test]
    fn forall_equal_fields_one_mismatch_is_false() {
        let data = map_of(&[(
            "items",
            Entity::List(vec![
                obj(&[("a", Entity::Int(1)), ("b", Entity::Int(1))]),
                obj(&[("a", Entity::Int(2)), ("b", Entity::Int(9))]),
            ]),
        )]);
        assert_eq!(run("(ForAll (EQ @.a @.b) .items)", data), ExecResult::False);
    }

    #[test]
    fn forall_sum_of_fields_equals_third_field() {
        let data = map_of(&[(
            "items",
            Entity::List(vec![
                obj(&[
                    ("a", Entity::Int(1)),
                    ("b", Entity::Int(1)),
                    ("c", Entity::Int(2)),
                ]),
                obj(&[
                    ("a", Entity::Int(2)),
                    ("b", Entity::Int(2)),
                    ("c", Entity::Int(4)),
                ]),
                obj(&[
                    ("a", Entity::Int(3)),
                    ("b", Entity::Int(3)),
                    ("c", Entity::Int(6)),
                ]),
            ]),
        )]);
        assert_eq!(
            run("(ForAll (EQ (Add @.a @.b) @.c) .items)", data),
            ExecResult::True
        );
    }

    #[test]
    fn bare_at_refers_to_whole_element() {
        let data = map_of(&[(
            "scores",
            Entity::List(vec![Entity::Int(1), Entity::Int(2), Entity::Int(3)]),
        )]);
        assert_eq!(run("(ForAll (GT @ 0) .scores)", data), ExecResult::True);
    }

    #[test]
    fn at_field_missing_is_symbol_not_found() {
        let data = map_of(&[(
            "items",
            Entity::List(vec![
                obj(&[("a", Entity::Int(1)), ("b", Entity::Int(1))]),
                obj(&[("a", Entity::Int(2))]), // missing `b`
            ]),
        )]);
        let r = run("(ForAll (EQ @.a @.b) .items)", data);
        assert!(matches!(
            r,
            ExecResult::Error(NightjarLanguageError::SymbolNotFound { .. })
        ));
    }

    #[test]
    fn mixed_root_and_element_symbols_in_predicate() {
        // Every employee's salary is above the root-level threshold.
        let data = map_of(&[
            ("threshold", Entity::Int(100)),
            (
                "employees",
                Entity::List(vec![
                    obj(&[("salary", Entity::Int(150))]),
                    obj(&[("salary", Entity::Int(200))]),
                ]),
            ),
        ]);
        assert_eq!(
            run("(ForAll (GT @.salary .threshold) .employees)", data),
            ExecResult::True
        );
    }

    #[test]
    fn nested_quantifier_inner_at_refers_to_inner_element() {
        // Two teams, each with a list of scores. Every score in every team > 0.
        let data = map_of(&[(
            "teams",
            Entity::List(vec![
                obj(&[("scores", Entity::List(vec![Entity::Int(1), Entity::Int(2)]))]),
                obj(&[("scores", Entity::List(vec![Entity::Int(3), Entity::Int(4)]))]),
            ]),
        )]);
        assert_eq!(
            run("(ForAll (ForAll (GT @ 0) @.scores) .teams)", data),
            ExecResult::True
        );
    }

    #[test]
    fn exists_with_full_predicate_short_circuits() {
        let data = map_of(&[(
            "items",
            Entity::List(vec![
                obj(&[("a", Entity::Int(1)), ("b", Entity::Int(2))]),
                obj(&[("a", Entity::Int(5)), ("b", Entity::Int(5))]),
                obj(&[("a", Entity::Int(9)), ("b", Entity::Int(8))]),
            ]),
        )]);
        assert_eq!(run("(Exists (EQ @.a @.b) .items)", data), ExecResult::True);
    }

    #[test]
    fn forall_full_predicate_on_empty_list_is_vacuously_true() {
        let data = map_of(&[("items", Entity::List(vec![]))]);
        assert_eq!(run("(ForAll (EQ @.a @.b) .items)", data), ExecResult::True);
    }

    #[test]
    fn depth_limit_surfaces_as_exec_error() {
        // Exercise the depth guard at a low limit so we don't have to push
        // the real Rust call stack close to its ceiling. 20 nested NOTs
        // against a max_depth of 10 must surface DepthLimitExceeded.
        let mut s = String::new();
        for _ in 0..20 {
            s.push_str("(NOT ");
        }
        s.push_str("True");
        for _ in 0..20 {
            s.push(')');
        }
        let opts = ExecOptions {
            max_depth: 10,
            ..ExecOptions::default()
        };
        let r = exec_entity(&s, empty(), opts);
        assert!(matches!(
            r,
            ExecResult::Error(NightjarLanguageError::RecursionError { .. })
        ));
    }
}
