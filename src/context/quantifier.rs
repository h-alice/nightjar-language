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
//

//! Implementations of the ForAll and Exists quantifiers.
//!
//! Including the scalar-fallback rule and the vacuously-true empty-list
//! (trivially true) semantics.

use crate::context::entity::Entity;
use crate::context::verifier::apply_verifier;
use crate::error::{type_error, NightjarLanguageError, Span};
use crate::language::grammar::{QuantifierOp, UnaryCheckOp, VerifierOp};

/// A resolved predicate — the bound value in a partial verifier has already
/// been reduced to an Entity.
#[derive(Debug, Clone)]
pub enum EvalPredicate {
    PartialVerifier { op: VerifierOp, bound: Entity },
    UnaryCheck(UnaryCheckOp),
}

/// Evaluate a quantifier (`ForAll` / `Exists`) over its operand.
///
/// - Operand must be `List`, `Map` is a `TypeError` (use `GetKeys` / `GetValues`).
/// - Scalar fallback: when the operand is a scalar or `Null`, the quantifier
///   reduces to a single predicate application.
/// - Empty list: `ForAll` is vacuously `true`, `Exists` is `false`.
pub fn apply_quantifier(
    op: QuantifierOp,
    predicate: &EvalPredicate,
    operand: &Entity,
    epsilon: f64,
    span: Span,
) -> Result<bool, NightjarLanguageError> {
    let elements: &[Entity] = match operand {
        Entity::List(v) => v,
        Entity::Map(_) => {
            return Err(type_error(
                span,
                "quantifier requires List operand; use GetKeys/GetValues for Maps",
            ));
        }
        // Scalar fallback: single predicate application.
        _scalar => {
            return apply_predicate(predicate, operand, epsilon, span);
        }
    };

    match op {
        QuantifierOp::ForAll => {
            for elem in elements {
                if !apply_predicate(predicate, elem, epsilon, span)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        QuantifierOp::Exists => {
            for elem in elements {
                if apply_predicate(predicate, elem, epsilon, span)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

/// Apply a unary boolean predicate to a single Entity.
///
/// For a partial verifier, the element fills the 1st operand and the bound
/// value fills the 2nd operand, i.e. `(ForAll (GT 2) [1,2,3])` means
/// `∀x. x > 2`.
pub fn apply_predicate(
    pred: &EvalPredicate,
    element: &Entity,
    epsilon: f64,
    span: Span,
) -> Result<bool, NightjarLanguageError> {
    match pred {
        EvalPredicate::PartialVerifier { op, bound } => {
            apply_verifier(*op, element, bound, epsilon, span)
        }
        EvalPredicate::UnaryCheck(UnaryCheckOp::NonEmpty) => Ok(element.is_non_empty()),
    }
}

/// Iterate a quantifier over `operand`, invoking `eval_on_element` once per
/// element with the element bound in scope.
///
/// Shares the list/scalar/map dispatch with [`apply_quantifier`].
///
/// Used for `Predicate::Full` where the predicate body is a general `BoolExpr`
/// re-evaluated per element.
pub fn apply_quantifier_full<F>(
    op: QuantifierOp,
    operand: &Entity,
    span: Span,
    mut eval_on_element: F, // A lambda that evaluates the predicate body with the element bound in scope.
) -> Result<bool, NightjarLanguageError>
where
    F: FnMut(&Entity) -> Result<bool, NightjarLanguageError>,
{
    let elements: &[Entity] = match operand {
        Entity::List(v) => v,
        Entity::Map(_) => {
            return Err(type_error(
                span,
                "quantifier requires List operand; use GetKeys/GetValues for Maps",
            ));
        }
        // Scalar fallback: single predicate application.
        scalar => return eval_on_element(scalar),
    };

    match op {
        QuantifierOp::ForAll => {
            for elem in elements {
                if !eval_on_element(elem)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        QuantifierOp::Exists => {
            for elem in elements {
                if eval_on_element(elem)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-10;

    fn gt(bound: i64) -> EvalPredicate {
        EvalPredicate::PartialVerifier {
            op: VerifierOp::GT,
            bound: Entity::Int(bound),
        }
    }

    fn eq_str(bound: &str) -> EvalPredicate {
        EvalPredicate::PartialVerifier {
            op: VerifierOp::EQ,
            bound: Entity::String(bound.to_string()),
        }
    }

    fn non_empty() -> EvalPredicate {
        EvalPredicate::UnaryCheck(UnaryCheckOp::NonEmpty)
    }

    fn list_of_ints(xs: &[i64]) -> Entity {
        Entity::List(xs.iter().copied().map(Entity::Int).collect())
    }

    // ── ForAll ───────────────────────────────────────────────

    #[test]
    fn forall_all_greater_than_zero() {
        let r = apply_quantifier(
            QuantifierOp::ForAll,
            &gt(0),
            &list_of_ints(&[1, 2, 3]),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(r);
    }

    #[test]
    fn forall_fails_when_zero_included() {
        let r = apply_quantifier(
            QuantifierOp::ForAll,
            &gt(0),
            &list_of_ints(&[0, 1, 2]),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(!r);
    }

    #[test]
    fn forall_empty_list_is_vacuously_true() {
        let r = apply_quantifier(
            QuantifierOp::ForAll,
            &gt(0),
            &list_of_ints(&[]),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(r);
    }

    // ── Exists ───────────────────────────────────────────────

    #[test]
    fn exists_finds_matching_value() {
        let r = apply_quantifier(
            QuantifierOp::Exists,
            &EvalPredicate::PartialVerifier {
                op: VerifierOp::EQ,
                bound: Entity::Int(2),
            },
            &list_of_ints(&[1, 2, 3]),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(r);
    }

    #[test]
    fn exists_returns_false_when_no_match() {
        let r = apply_quantifier(
            QuantifierOp::Exists,
            &gt(10),
            &list_of_ints(&[1, 2, 3]),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(!r);
    }

    #[test]
    fn exists_empty_list_is_false() {
        let r = apply_quantifier(
            QuantifierOp::Exists,
            &gt(0),
            &list_of_ints(&[]),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(!r);
    }

    // ── Predicates ───────────────────────────────────────────

    #[test]
    fn exists_admin_role_in_string_list() {
        let list = Entity::List(vec![
            Entity::String("user".into()),
            Entity::String("admin".into()),
        ]);
        let r = apply_quantifier(
            QuantifierOp::Exists,
            &eq_str("admin"),
            &list,
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(r);
    }

    #[test]
    fn forall_nonempty_strings() {
        let bad = Entity::List(vec![
            Entity::String("a".into()),
            Entity::String("".into()),
            Entity::String("b".into()),
        ]);
        assert!(!apply_quantifier(
            QuantifierOp::ForAll,
            &non_empty(),
            &bad,
            EPS,
            Span::new(0, 0),
        )
        .unwrap());

        let good = Entity::List(vec![Entity::String("a".into()), Entity::String("b".into())]);
        assert!(apply_quantifier(
            QuantifierOp::ForAll,
            &non_empty(),
            &good,
            EPS,
            Span::new(0, 0),
        )
        .unwrap());
    }

    #[test]
    fn exists_nonempty_in_all_empty_strings_is_false() {
        let all_empty = Entity::List(vec![Entity::String("".into()), Entity::String("".into())]);
        assert!(!apply_quantifier(
            QuantifierOp::Exists,
            &non_empty(),
            &all_empty,
            EPS,
            Span::new(0, 0),
        )
        .unwrap());
    }

    // ── Fallback / errors ────────────────────────────────────

    #[test]
    fn scalar_fallback_reduces_to_predicate() {
        // (ForAll (GT 0) 5) → True
        let r = apply_quantifier(
            QuantifierOp::ForAll,
            &gt(0),
            &Entity::Int(5),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(r);

        // (ForAll (GT 0) -1) → False
        let r = apply_quantifier(
            QuantifierOp::ForAll,
            &gt(0),
            &Entity::Int(-1),
            EPS,
            Span::new(0, 0),
        )
        .unwrap();
        assert!(!r);
    }

    #[test]
    fn map_operand_is_type_error() {
        let m = Entity::Map(std::collections::HashMap::new());
        let err =
            apply_quantifier(QuantifierOp::ForAll, &gt(0), &m, EPS, Span::new(0, 0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn predicate_type_error_propagates() {
        // Comparing Int > String is a type error — when iterating, the first
        // element to hit that should surface the error up.
        let bad = Entity::List(vec![Entity::String("a".into())]);
        let err =
            apply_quantifier(QuantifierOp::ForAll, &gt(0), &bad, EPS, Span::new(0, 0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    // ── apply_quantifier_full ────────────────────────────────

    #[test]
    fn full_forall_all_true() {
        let r = apply_quantifier_full(
            QuantifierOp::ForAll,
            &list_of_ints(&[1, 2, 3]),
            Span::new(0, 0),
            |e| Ok(matches!(e, Entity::Int(i) if *i > 0)),
        )
        .unwrap();
        assert!(r);
    }

    #[test]
    fn full_forall_short_circuits_on_false() {
        let mut calls = 0;
        let r = apply_quantifier_full(
            QuantifierOp::ForAll,
            &list_of_ints(&[1, 0, 2]),
            Span::new(0, 0),
            |e| {
                calls += 1;
                Ok(matches!(e, Entity::Int(i) if *i > 0))
            },
        )
        .unwrap();
        assert!(!r);
        assert_eq!(calls, 2, "ForAll should short-circuit at first false");
    }

    #[test]
    fn full_exists_short_circuits_on_true() {
        let mut calls = 0;
        let r = apply_quantifier_full(
            QuantifierOp::Exists,
            &list_of_ints(&[0, 1, 2]),
            Span::new(0, 0),
            |e| {
                calls += 1;
                Ok(matches!(e, Entity::Int(i) if *i > 0))
            },
        )
        .unwrap();
        assert!(r);
        assert_eq!(calls, 2, "Exists should short-circuit at first true");
    }

    #[test]
    fn full_empty_list_semantics() {
        let empty = list_of_ints(&[]);
        assert!(
            apply_quantifier_full(QuantifierOp::ForAll, &empty, Span::new(0, 0), |_| Ok(false))
                .unwrap()
        );
        assert!(
            !apply_quantifier_full(QuantifierOp::Exists, &empty, Span::new(0, 0), |_| Ok(true))
                .unwrap()
        );
    }

    #[test]
    fn full_scalar_fallback_invokes_once() {
        let mut calls = 0;
        let r = apply_quantifier_full(
            QuantifierOp::ForAll,
            &Entity::Int(5),
            Span::new(0, 0),
            |e| {
                calls += 1;
                Ok(matches!(e, Entity::Int(i) if *i > 0))
            },
        )
        .unwrap();
        assert!(r);
        assert_eq!(calls, 1);
    }

    #[test]
    fn full_map_operand_is_type_error() {
        let err = apply_quantifier_full(
            QuantifierOp::ForAll,
            &Entity::Map(std::collections::HashMap::new()),
            Span::new(0, 0),
            |_| Ok(true),
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }
}
