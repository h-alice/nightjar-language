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

//! Implementation of the binary verifiers (EQ, NE, LT, LE, GT, GE)
//!
//! Also including epsilon-based float equality and NaN handling.

use std::cmp::Ordering;

use crate::context::entity::Entity;
use crate::error::{type_error, NightjarLanguageError, Span};
use crate::language::grammar::VerifierOp;

/// Apply a binary verifier to two entities.
///
/// Semantics:
/// - No implicit coercion **except** Int↔Float promotion in numeric contexts.
/// - EQ/NE on `Float` (or after Int→Float promotion) uses epsilon-based
///   comparison with `epsilon` as the tolerance.
/// - Any comparison involving `NaN` returns `Ok(false)`
/// - Strings compare lexicographically.
/// - Booleans and Null have equality only.
///
/// Incompatible type pairs (e.g. Int vs String) produce a `TypeError`.
pub fn apply_verifier(
    op: VerifierOp,
    left: &Entity,
    right: &Entity,
    epsilon: f64, // A small tolerance for float comparison.
    span: Span,
) -> Result<bool, NightjarLanguageError> {
    match op {
        VerifierOp::EQ => eq(left, right, epsilon, span),
        VerifierOp::NE => eq(left, right, epsilon, span).map(|b| !b),
        VerifierOp::LT => Ok(ordering(left, right, span)?
            .map(|o| o == Ordering::Less)
            .unwrap_or(false)),
        VerifierOp::LE => Ok(ordering(left, right, span)?
            .map(|o| o != Ordering::Greater)
            .unwrap_or(false)),
        VerifierOp::GT => Ok(ordering(left, right, span)?
            .map(|o| o == Ordering::Greater)
            .unwrap_or(false)),
        VerifierOp::GE => Ok(ordering(left, right, span)?
            .map(|o| o != Ordering::Less)
            .unwrap_or(false)),
    }
}

/// Implementation of equality check.
///
/// Structural / numeric equality helper used by `EQ` and `NE`. Int ↔ Float
/// comparisons auto-promote to Float with epsilon-tolerance via
/// [`float_eq`].
///
/// `List` and `Map` equality delegates to Rust's derived `PartialEq`,
/// so structural equality of children is required.
///
/// Mismatched types outside that small auto-promotion set produce a
/// `TypeError`.
///
/// Example (internal):
///
/// ```ignore
/// use crate::context::verifier::eq;
/// use crate::context::entity::Entity;
/// use crate::error::Span;
///
/// assert!(eq(&Entity::Int(3), &Entity::Int(3), 1e-10, Span::point(0)).unwrap());
/// // Int vs Float is allowed (auto-promotion)
/// assert!(eq(&Entity::Int(3), &Entity::Float(3.0), 1e-10, Span::point(0)).unwrap());
/// ```
fn eq(a: &Entity, b: &Entity, epsilon: f64, span: Span) -> Result<bool, NightjarLanguageError> {
    match (a, b) {
        (Entity::Int(x), Entity::Int(y)) => Ok(x == y),
        (Entity::Float(x), Entity::Float(y)) => Ok(float_eq(*x, *y, epsilon)),
        // Auto-promote Int→Float for cross-type numeric equality.
        (Entity::Int(x), Entity::Float(y)) => Ok(float_eq(*x as f64, *y, epsilon)),
        (Entity::Float(x), Entity::Int(y)) => Ok(float_eq(*x, *y as f64, epsilon)),
        (Entity::String(x), Entity::String(y)) => Ok(x == y),
        (Entity::Bool(x), Entity::Bool(y)) => Ok(x == y),
        (Entity::Null, Entity::Null) => Ok(true),
        // Structural equality on containers is allowed (uses type-tag matching).
        (Entity::List(x), Entity::List(y)) => Ok(x == y),
        (Entity::Map(x), Entity::Map(y)) => Ok(x == y),
        (l, r) => Err(type_error(
            span,
            format!(
                "cannot compare {} with {} for equality",
                l.type_tag(),
                r.type_tag()
            ),
        )),
    }
}

/// Special equality check for float.
///
/// Epsilon-tolerant float equality.
///
/// Equality of NaN values is always false.
///
/// Example (internal):
///
/// ```ignore
/// use crate::context::verifier::float_eq;
///
/// assert!(float_eq(0.1 + 0.2, 0.3, 1e-10));
/// assert!(!float_eq(f64::NAN, f64::NAN, 1e-10));
/// ```
fn float_eq(a: f64, b: f64, epsilon: f64) -> bool {
    // NaN-safe: (NaN - NaN).abs() is NaN, and produces false in the comparison.
    (a - b).abs() < epsilon
}

/// Compare and return the order of two comparable entities.
///
/// Returns `Ok(None)` when the pair involves NaN (unordered), `Ok(Some(..))`
/// for a valid ordering, or `Err(TypeError)` when the types don't support
/// ordering.
///
/// This function is used by `LT`/`LE`/`GT`/`GE`, callers fold `None` into
/// `false` to handle NaN comparisons.
///
/// Example (internal):
///
/// ```ignore
/// use std::cmp::Ordering;
/// use crate::context::verifier::ordering;
/// use crate::context::entity::Entity;
/// use crate::error::Span;
///
/// assert_eq!(
///     ordering(&Entity::Int(1), &Entity::Int(2), Span::point(0)).unwrap(),
///     Some(Ordering::Less),
/// );
/// // NaN is unordered — None rather than an error.
/// assert!(ordering(&Entity::Float(f64::NAN), &Entity::Float(1.0), Span::point(0))
///     .unwrap()
///     .is_none());
/// ```
fn ordering(a: &Entity, b: &Entity, span: Span) -> Result<Option<Ordering>, NightjarLanguageError> {
    match (a, b) {
        (Entity::Int(x), Entity::Int(y)) => Ok(Some(x.cmp(y))),
        (Entity::Float(x), Entity::Float(y)) => Ok(x.partial_cmp(y)),
        (Entity::Int(x), Entity::Float(y)) => Ok((*x as f64).partial_cmp(y)),
        (Entity::Float(x), Entity::Int(y)) => Ok(x.partial_cmp(&(*y as f64))),
        (Entity::String(x), Entity::String(y)) => Ok(Some(x.cmp(y))), // lexicographical order
        (l, r) => Err(type_error(
            span,
            format!("cannot order {} with {}", l.type_tag(), r.type_tag()),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn v(op: VerifierOp, a: Entity, b: Entity) -> Result<bool, NightjarLanguageError> {
        apply_verifier(op, &a, &b, EPS, Span::new(0, 1))
    }

    // ── EQ / NE ──────────────────────────────────────────────

    #[test]
    fn eq_int_int() {
        assert!(v(VerifierOp::EQ, Entity::Int(1), Entity::Int(1)).unwrap());
        assert!(!v(VerifierOp::EQ, Entity::Int(1), Entity::Int(2)).unwrap());
    }

    #[test]
    fn eq_float_with_epsilon() {
        // 0.1 + 0.2 != 0.3 under IEEE exact equality, but should be EQ under epsilon.
        let lhs = Entity::Float(0.1 + 0.2);
        let rhs = Entity::Float(0.3);
        assert!(v(VerifierOp::EQ, lhs, rhs).unwrap());
    }

    #[test]
    fn eq_int_float_promotes() {
        assert!(v(VerifierOp::EQ, Entity::Int(2), Entity::Float(2.0)).unwrap());
        assert!(!v(VerifierOp::EQ, Entity::Int(2), Entity::Float(2.5)).unwrap());
    }

    #[test]
    fn eq_strings_and_bools_and_nulls() {
        assert!(v(
            VerifierOp::EQ,
            Entity::String("a".into()),
            Entity::String("a".into())
        )
        .unwrap());
        assert!(!v(
            VerifierOp::EQ,
            Entity::String("a".into()),
            Entity::String("b".into())
        )
        .unwrap());
        assert!(v(VerifierOp::EQ, Entity::Bool(true), Entity::Bool(true)).unwrap());
        assert!(v(VerifierOp::EQ, Entity::Null, Entity::Null).unwrap());
    }

    #[test]
    fn ne_is_negation_of_eq() {
        assert!(!v(VerifierOp::NE, Entity::Int(1), Entity::Int(1)).unwrap());
        assert!(v(VerifierOp::NE, Entity::Int(1), Entity::Int(2)).unwrap());
    }

    #[test]
    fn eq_incompatible_types_error() {
        let err = v(VerifierOp::EQ, Entity::Int(1), Entity::String("x".into())).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn eq_bool_vs_int_is_type_error() {
        // Booleans and ints are distinct types — comparing them is an error
        // (per the PRD's strict no-coercion rule outside Int↔Float).
        let err = v(VerifierOp::EQ, Entity::Bool(true), Entity::Int(1)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    // ── Ordering ─────────────────────────────────────────────

    #[test]
    fn ordering_int() {
        assert!(v(VerifierOp::LT, Entity::Int(1), Entity::Int(2)).unwrap());
        assert!(!v(VerifierOp::LT, Entity::Int(2), Entity::Int(2)).unwrap());
        assert!(v(VerifierOp::LE, Entity::Int(2), Entity::Int(2)).unwrap());
        assert!(v(VerifierOp::GT, Entity::Int(3), Entity::Int(2)).unwrap());
        assert!(v(VerifierOp::GE, Entity::Int(2), Entity::Int(2)).unwrap());
    }

    #[test]
    fn ordering_float() {
        assert!(v(VerifierOp::LT, Entity::Float(1.0), Entity::Float(2.0)).unwrap());
        assert!(v(VerifierOp::GT, Entity::Float(2.5), Entity::Float(2.0)).unwrap());
    }

    #[test]
    fn ordering_int_float_cross() {
        assert!(v(VerifierOp::GT, Entity::Int(3), Entity::Float(2.5)).unwrap());
        assert!(v(VerifierOp::LT, Entity::Float(2.5), Entity::Int(3)).unwrap());
    }

    #[test]
    fn ordering_strings_lexicographic() {
        assert!(v(
            VerifierOp::LT,
            Entity::String("abc".into()),
            Entity::String("abd".into())
        )
        .unwrap());
        assert!(v(
            VerifierOp::GE,
            Entity::String("abc".into()),
            Entity::String("abc".into())
        )
        .unwrap());
    }

    #[test]
    fn ordering_type_error_for_bool() {
        let err = v(VerifierOp::LT, Entity::Bool(false), Entity::Bool(true)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn ordering_type_error_for_null() {
        let err = v(VerifierOp::GT, Entity::Null, Entity::Int(0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    // ── NaN ──────────────────────────────────────────────────

    #[test]
    fn nan_ordering_always_false() {
        let nan = Entity::Float(f64::NAN);
        assert!(!v(VerifierOp::LT, nan.clone(), Entity::Float(1.0)).unwrap());
        assert!(!v(VerifierOp::LE, nan.clone(), Entity::Float(1.0)).unwrap());
        assert!(!v(VerifierOp::GT, nan.clone(), Entity::Float(1.0)).unwrap());
        assert!(!v(VerifierOp::GE, nan.clone(), Entity::Float(1.0)).unwrap());
    }

    #[test]
    fn nan_eq_false_ne_true() {
        let nan = Entity::Float(f64::NAN);
        assert!(!v(VerifierOp::EQ, nan.clone(), nan.clone()).unwrap());
        assert!(v(VerifierOp::NE, nan.clone(), nan.clone()).unwrap());
    }

    #[test]
    fn configurable_epsilon_affects_eq() {
        let lhs = Entity::Float(1.0);
        let rhs = Entity::Float(1.0 + 1e-6);
        // With tight epsilon they are not equal.
        let tight = apply_verifier(VerifierOp::EQ, &lhs, &rhs, 1e-12, Span::new(0, 0)).unwrap();
        assert!(!tight);
        // With loose epsilon they are equal.
        let loose = apply_verifier(VerifierOp::EQ, &lhs, &rhs, 1e-3, Span::new(0, 0)).unwrap();
        assert!(loose);
    }

    #[test]
    fn eq_lists_structurally() {
        assert!(v(
            VerifierOp::EQ,
            Entity::List(vec![Entity::Int(1), Entity::Int(2)]),
            Entity::List(vec![Entity::Int(1), Entity::Int(2)])
        )
        .unwrap());
    }
}
