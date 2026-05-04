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

//! Implementation of the function operators.
//!
//! Runtime implementations of the function operators (arithmetic, string,
//! collection) with type-directed dispatch, Int -> Float promotion, and
//! overflow-safe semantics.

use crate::context::entity::Entity;
use crate::error::{
    argument_error, division_by_zero, index_error, integer_overflow, symbol_not_found, type_error,
    NightjarLanguageError, Span,
};
use crate::language::grammar::FuncOp;

/// Normalize a pair of numeric operands under the Int -> Float auto-promotion rule.
enum NumericPair {
    IntInt(i64, i64),
    FloatFloat(f64, f64),
}

/// Numeric operands normalization.
///
/// Normalize a pair of numeric operands into either `IntInt` (both `i64`)
/// or `FloatFloat` (both `f64`, with any `Int` side auto-promoted).
///
/// Any non-numeric operand (or mixing numeric with `String`/`Bool`/etc.) is a
/// `TypeError` carrying `op` as the operator name for diagnostics.
///
/// Example (internal):
///
/// ```ignore
/// use crate::context::function::numeric_pair;
/// use crate::context::entity::Entity;
/// use crate::error::Span;
///
/// // Int + Int stays integer.
/// let pair = numeric_pair(&Entity::Int(1), &Entity::Int(2), Span::point(0), "Add").unwrap();
/// // Int + Float promotes to Float.
/// let pair = numeric_pair(&Entity::Int(1), &Entity::Float(2.0), Span::point(0), "Add").unwrap();
/// // String + Int is a TypeError carrying the op name.
/// assert!(numeric_pair(&Entity::String("x".into()), &Entity::Int(1), Span::point(0), "Add").is_err());
/// ```
fn numeric_pair(
    a: &Entity,
    b: &Entity,
    span: Span,
    op: &str,
) -> Result<NumericPair, NightjarLanguageError> {
    match (a, b) {
        (Entity::Int(x), Entity::Int(y)) => Ok(NumericPair::IntInt(*x, *y)),
        (Entity::Float(x), Entity::Float(y)) => Ok(NumericPair::FloatFloat(*x, *y)),

        // Auto-promotion from Int to Float.
        (Entity::Int(x), Entity::Float(y)) => Ok(NumericPair::FloatFloat(*x as f64, *y)),
        (Entity::Float(x), Entity::Int(y)) => Ok(NumericPair::FloatFloat(*x, *y as f64)),
        (l, r) => Err(type_error(
            span,
            format!(
                "`{}` expects numeric operands, got {} and {}",
                op,
                l.type_tag(),
                r.type_tag()
            ),
        )),
    }
}

/// Execute a FuncOp against its evaluated arguments.
pub fn apply_function(
    op: FuncOp,
    args: Vec<Entity>,
    span: Span,
) -> Result<Entity, NightjarLanguageError> {
    // Double check arity.
    //
    // The parser already enforces this, but we keep the invariant locally so direct
    // callers can't smuggle in a bad arity.
    let expected = op.expected_arity();
    if args.len() != expected {
        return Err(argument_error(
            span,
            format!(
                "`{}` expects {} argument(s), got {}",
                op.name(),
                expected,
                args.len()
            ),
        ));
    }

    match op {
        // ───────────────────────── Arithmetic ──────────────────────────
        FuncOp::Add => match numeric_pair(&args[0], &args[1], span, "Add")? {
            NumericPair::IntInt(x, y) => x
                .checked_add(y)
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Add")),
            NumericPair::FloatFloat(x, y) => Ok(Entity::Float(x + y)),
        },
        FuncOp::Sub => match numeric_pair(&args[0], &args[1], span, "Sub")? {
            NumericPair::IntInt(x, y) => x
                .checked_sub(y)
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Sub")),
            NumericPair::FloatFloat(x, y) => Ok(Entity::Float(x - y)),
        },
        FuncOp::Mul => match numeric_pair(&args[0], &args[1], span, "Mul")? {
            NumericPair::IntInt(x, y) => x
                .checked_mul(y)
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Mul")),
            NumericPair::FloatFloat(x, y) => Ok(Entity::Float(x * y)),
        },
        FuncOp::Div => match numeric_pair(&args[0], &args[1], span, "Div")? {
            NumericPair::IntInt(_, 0) => Err(division_by_zero(span)),
            NumericPair::IntInt(x, y) => x
                .checked_div(y)
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Div")),
            NumericPair::FloatFloat(x, y) => { // Fix redundant guard
                if y == 0.0 {
                    Err(division_by_zero(span))
                } else {
                    Ok(Entity::Float(x / y))
                }
            }
        },
        FuncOp::Mod => match numeric_pair(&args[0], &args[1], span, "Mod")? {
            NumericPair::IntInt(_, 0) => Err(division_by_zero(span)),
            NumericPair::IntInt(x, y) => x
                .checked_rem(y)
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Mod")),
            NumericPair::FloatFloat(x, y) => { // Fix redundant guard
                if y == 0.0 {
                    Err(division_by_zero(span))
                } else {
                    Ok(Entity::Float(x % y))
                }
            }
        },
        FuncOp::Neg => match &args[0] {
            Entity::Int(x) => x
                .checked_neg()
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Neg")),
            Entity::Float(x) => Ok(Entity::Float(-x)),
            other => Err(type_error(
                span,
                format!("`Neg` expects numeric operand, got {}", other.type_tag()),
            )),
        },
        FuncOp::Abs => match &args[0] {
            Entity::Int(x) => x
                .checked_abs()
                .map(Entity::Int)
                .ok_or_else(|| integer_overflow(span, "Abs")),
            Entity::Float(x) => Ok(Entity::Float(x.abs())),
            other => Err(type_error(
                span,
                format!("`Abs` expects numeric operand, got {}", other.type_tag()),
            )),
        },

        // ─────────────────────────── String ───────────────────────────
        FuncOp::Concat => match (&args[0], &args[1]) {
            (Entity::String(a), Entity::String(b)) => Ok(Entity::String(format!("{}{}", a, b))),
            (l, r) => Err(type_error(
                span,
                format!(
                    "`Concat` expects two Strings, got {} and {}",
                    l.type_tag(),
                    r.type_tag()
                ),
            )),
        },
        FuncOp::Length => match &args[0] {
            Entity::String(s) => Ok(Entity::Int(s.chars().count() as i64)),
            other => Err(type_error(
                span,
                format!("`Length` expects String, got {}", other.type_tag()),
            )),
        },
        FuncOp::Substring => match (&args[0], &args[1], &args[2]) {
            (Entity::String(s), Entity::Int(start), Entity::Int(len)) => {
                if *start < 0 || *len < 0 {
                    return Err(type_error(
                        span,
                        "`Substring` requires non-negative start and length",
                    ));
                }
                let start = *start as usize;
                let len = *len as usize;
                let result: String = s.chars().skip(start).take(len).collect();
                Ok(Entity::String(result))
            }
            (l, r, len) => Err(type_error(
                span,
                format!(
                    "`Substring` expects (String, Int, Int), got ({}, {}, {})",
                    l.type_tag(),
                    r.type_tag(),
                    len.type_tag()
                ),
            )),
        },
        FuncOp::Upper => match &args[0] {
            Entity::String(s) => Ok(Entity::String(s.to_uppercase())),
            other => Err(type_error(
                span,
                format!("`Upper` expects String, got {}", other.type_tag()),
            )),
        },
        FuncOp::Lower => match &args[0] {
            Entity::String(s) => Ok(Entity::String(s.to_lowercase())),
            other => Err(type_error(
                span,
                format!("`Lower` expects String, got {}", other.type_tag()),
            )),
        },

        // ───────────────────────── Collection ─────────────────────────
        FuncOp::Head => match &args[0] {
            Entity::List(v) if v.is_empty() => Err(type_error(span, "`Head` of empty list")),
            Entity::List(v) => Ok(v[0].clone()),
            other => Err(type_error(
                span,
                format!("`Head` expects List, got {}", other.type_tag()),
            )),
        },
        FuncOp::Tail => match &args[0] {
            Entity::List(v) if v.is_empty() => Err(type_error(span, "`Tail` of empty list")),
            Entity::List(v) => Ok(Entity::List(v[1..].to_vec())),
            other => Err(type_error(
                span,
                format!("`Tail` expects List, got {}", other.type_tag()),
            )),
        },
        FuncOp::Get => match (&args[0], &args[1]) {
            (Entity::List(v), Entity::Int(i)) => {
                if *i < 0 {
                    return Err(index_error(span, *i, v.len()));
                }
                let idx = *i as usize;
                v.get(idx)
                    .cloned()
                    .ok_or_else(|| index_error(span, *i, v.len()))
            }
            (Entity::Map(m), Entity::String(k)) => {
                m.get(k).cloned().ok_or_else(|| symbol_not_found(span, k))
            }
            (l, r) => Err(type_error(
                span,
                format!(
                    "`Get` expects (List, Int) or (Map, String), got ({}, {})",
                    l.type_tag(),
                    r.type_tag()
                ),
            )),
        },
        FuncOp::Count => match &args[0] {
            Entity::List(v) => Ok(Entity::Int(v.len() as i64)),
            Entity::Map(m) => Ok(Entity::Int(m.len() as i64)),
            other => Err(type_error(
                span,
                format!("`Count` expects List or Map, got {}", other.type_tag()),
            )),
        },
        FuncOp::GetKeys => match &args[0] {
            Entity::Map(m) => {
                let mut keys: Vec<Entity> = m.keys().map(|k| Entity::String(k.clone())).collect();
                // Order is unspecified for HashMap; sort keys so downstream
                // quantifiers / tests see a deterministic result. This does
                // not affect semantics — quantifiers don't care about order.
                keys.sort_by(|a, b| match (a, b) {
                    (Entity::String(x), Entity::String(y)) => x.cmp(y),
                    _ => std::cmp::Ordering::Equal,
                });
                Ok(Entity::List(keys))
            }
            other => Err(type_error(
                span,
                format!("`GetKeys` expects Map, got {}", other.type_tag()),
            )),
        },
        FuncOp::GetValues => match &args[0] {
            Entity::Map(m) => {
                // Pair keys with values, sort by key for deterministic order,
                // then extract values.
                let mut pairs: Vec<(&String, &Entity)> = m.iter().collect();
                pairs.sort_by(|a, b| a.0.cmp(b.0));
                let values: Vec<Entity> = pairs.into_iter().map(|(_, v)| v.clone()).collect();
                Ok(Entity::List(values))
            }
            other => Err(type_error(
                span,
                format!("`GetValues` expects Map, got {}", other.type_tag()),
            )),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::NightjarLanguageError;
    use std::collections::HashMap;

    fn sp() -> Span {
        Span::new(0, 1)
    }

    fn call(op: FuncOp, args: Vec<Entity>) -> Result<Entity, NightjarLanguageError> {
        apply_function(op, args, sp())
    }

    // ── Arithmetic ───────────────────────────────────────────

    #[test]
    fn add_int_int() {
        assert_eq!(
            call(FuncOp::Add, vec![Entity::Int(1), Entity::Int(2)]).unwrap(),
            Entity::Int(3)
        );
    }

    #[test]
    fn add_promotes_int_to_float() {
        assert_eq!(
            call(FuncOp::Add, vec![Entity::Int(1), Entity::Float(2.0)]).unwrap(),
            Entity::Float(3.0)
        );
        assert_eq!(
            call(FuncOp::Add, vec![Entity::Float(1.5), Entity::Int(2)]).unwrap(),
            Entity::Float(3.5)
        );
    }

    #[test]
    fn add_rejects_string() {
        let err = call(
            FuncOp::Add,
            vec![Entity::Int(1), Entity::String("x".into())],
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn add_overflow_errors() {
        let err = call(FuncOp::Add, vec![Entity::Int(i64::MAX), Entity::Int(1)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IntegerOverflow { .. }));
    }

    #[test]
    fn sub_int_and_float() {
        assert_eq!(
            call(FuncOp::Sub, vec![Entity::Int(5), Entity::Int(3)]).unwrap(),
            Entity::Int(2)
        );
        assert_eq!(
            call(FuncOp::Sub, vec![Entity::Float(1.0), Entity::Float(0.5)]).unwrap(),
            Entity::Float(0.5)
        );
    }

    #[test]
    fn mul_overflow_errors() {
        let err = call(FuncOp::Mul, vec![Entity::Int(i64::MAX), Entity::Int(2)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IntegerOverflow { .. }));
    }

    #[test]
    fn div_int_is_integer_division() {
        assert_eq!(
            call(FuncOp::Div, vec![Entity::Int(7), Entity::Int(2)]).unwrap(),
            Entity::Int(3)
        );
    }

    #[test]
    fn div_mixed_promotes_to_float() {
        let r = call(FuncOp::Div, vec![Entity::Int(7), Entity::Float(2.0)]).unwrap();
        if let Entity::Float(f) = r {
            assert!((f - 3.5).abs() < 1e-12);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn div_by_zero_int_errors() {
        let err = call(FuncOp::Div, vec![Entity::Int(1), Entity::Int(0)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::DivisionByZero { .. }));
    }

    #[test]
    fn div_by_zero_float_errors() {
        let err = call(FuncOp::Div, vec![Entity::Float(1.0), Entity::Float(0.0)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::DivisionByZero { .. }));
    }

    #[test]
    fn div_overflow_i64min_by_neg_one() {
        let err = call(FuncOp::Div, vec![Entity::Int(i64::MIN), Entity::Int(-1)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IntegerOverflow { .. }));
    }

    #[test]
    fn mod_int_and_float() {
        assert_eq!(
            call(FuncOp::Mod, vec![Entity::Int(10), Entity::Int(3)]).unwrap(),
            Entity::Int(1)
        );
        let r = call(FuncOp::Mod, vec![Entity::Float(3.5), Entity::Float(1.5)]).unwrap();
        if let Entity::Float(f) = r {
            assert!((f - 0.5).abs() < 1e-12);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn mod_by_zero_errors() {
        let err = call(FuncOp::Mod, vec![Entity::Int(1), Entity::Int(0)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::DivisionByZero { .. }));
    }

    #[test]
    fn neg_int_and_float() {
        assert_eq!(
            call(FuncOp::Neg, vec![Entity::Int(5)]).unwrap(),
            Entity::Int(-5)
        );
        assert_eq!(
            call(FuncOp::Neg, vec![Entity::Float(-2.5)]).unwrap(),
            Entity::Float(2.5)
        );
    }

    #[test]
    fn neg_i64min_errors() {
        let err = call(FuncOp::Neg, vec![Entity::Int(i64::MIN)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IntegerOverflow { .. }));
    }

    #[test]
    fn abs_int_and_float() {
        assert_eq!(
            call(FuncOp::Abs, vec![Entity::Int(-3)]).unwrap(),
            Entity::Int(3)
        );
        assert_eq!(
            call(FuncOp::Abs, vec![Entity::Float(-1.5)]).unwrap(),
            Entity::Float(1.5)
        );
    }

    #[test]
    fn abs_i64min_errors() {
        let err = call(FuncOp::Abs, vec![Entity::Int(i64::MIN)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IntegerOverflow { .. }));
    }

    // ── String ───────────────────────────────────────────────

    #[test]
    fn concat_strings() {
        assert_eq!(
            call(
                FuncOp::Concat,
                vec![Entity::String("ab".into()), Entity::String("cd".into())]
            )
            .unwrap(),
            Entity::String("abcd".into())
        );
    }

    #[test]
    fn concat_rejects_non_strings() {
        let err = call(
            FuncOp::Concat,
            vec![Entity::Int(1), Entity::String("a".into())],
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn length_counts_chars_not_bytes() {
        assert_eq!(
            call(FuncOp::Length, vec![Entity::String("abc".into())]).unwrap(),
            Entity::Int(3)
        );
        assert_eq!(
            call(FuncOp::Length, vec![Entity::String("營收".into())]).unwrap(),
            Entity::Int(2)
        );
    }

    #[test]
    fn substring_basic_and_unicode() {
        assert_eq!(
            call(
                FuncOp::Substring,
                vec![
                    Entity::String("hello".into()),
                    Entity::Int(1),
                    Entity::Int(3),
                ],
            )
            .unwrap(),
            Entity::String("ell".into())
        );
        assert_eq!(
            call(
                FuncOp::Substring,
                vec![
                    Entity::String("こんにちは".into()),
                    Entity::Int(1),
                    Entity::Int(3),
                ],
            )
            .unwrap(),
            Entity::String("んにち".into())
        );
    }

    #[test]
    fn substring_out_of_range_clamps() {
        assert_eq!(
            call(
                FuncOp::Substring,
                vec![Entity::String("ab".into()), Entity::Int(5), Entity::Int(10),],
            )
            .unwrap(),
            Entity::String("".into())
        );
        assert_eq!(
            call(
                FuncOp::Substring,
                vec![
                    Entity::String("abcd".into()),
                    Entity::Int(1),
                    Entity::Int(100),
                ],
            )
            .unwrap(),
            Entity::String("bcd".into())
        );
    }

    #[test]
    fn substring_negative_errors() {
        let err = call(
            FuncOp::Substring,
            vec![Entity::String("hi".into()), Entity::Int(-1), Entity::Int(2)],
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn upper_lower_unicode_aware() {
        assert_eq!(
            call(FuncOp::Upper, vec![Entity::String("abc".into())]).unwrap(),
            Entity::String("ABC".into())
        );
        assert_eq!(
            call(FuncOp::Lower, vec![Entity::String("ABC".into())]).unwrap(),
            Entity::String("abc".into())
        );
        // Rust's to_uppercase expands ß to SS.
        assert_eq!(
            call(FuncOp::Upper, vec![Entity::String("ß".into())]).unwrap(),
            Entity::String("SS".into())
        );
    }

    // ── Collection ───────────────────────────────────────────

    #[test]
    fn head_first_element() {
        assert_eq!(
            call(
                FuncOp::Head,
                vec![Entity::List(vec![Entity::Int(1), Entity::Int(2)])]
            )
            .unwrap(),
            Entity::Int(1)
        );
    }

    #[test]
    fn head_empty_errors() {
        let err = call(FuncOp::Head, vec![Entity::List(vec![])]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn tail_drops_first() {
        assert_eq!(
            call(
                FuncOp::Tail,
                vec![Entity::List(vec![
                    Entity::Int(1),
                    Entity::Int(2),
                    Entity::Int(3)
                ])]
            )
            .unwrap(),
            Entity::List(vec![Entity::Int(2), Entity::Int(3)])
        );
    }

    #[test]
    fn tail_empty_errors() {
        let err = call(FuncOp::Tail, vec![Entity::List(vec![])]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn get_list_by_int() {
        assert_eq!(
            call(
                FuncOp::Get,
                vec![
                    Entity::List(vec![Entity::Int(10), Entity::Int(20), Entity::Int(30)]),
                    Entity::Int(1),
                ]
            )
            .unwrap(),
            Entity::Int(20)
        );
    }

    #[test]
    fn get_list_negative_index_errors() {
        let err = call(
            FuncOp::Get,
            vec![Entity::List(vec![Entity::Int(1)]), Entity::Int(-1)],
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IndexError { .. }));
    }

    #[test]
    fn get_list_out_of_range_errors() {
        let err = call(
            FuncOp::Get,
            vec![Entity::List(vec![Entity::Int(1)]), Entity::Int(5)],
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::IndexError { .. }));
    }

    #[test]
    fn get_map_by_string() {
        let mut m = HashMap::new();
        m.insert("key".to_string(), Entity::Int(42));
        assert_eq!(
            call(
                FuncOp::Get,
                vec![Entity::Map(m), Entity::String("key".into())]
            )
            .unwrap(),
            Entity::Int(42)
        );
    }

    #[test]
    fn get_map_missing_key_errors() {
        let m = HashMap::new();
        let err = call(
            FuncOp::Get,
            vec![Entity::Map(m), Entity::String("missing".into())],
        )
        .unwrap_err();
        assert!(matches!(err, NightjarLanguageError::SymbolNotFound { .. }));
    }

    #[test]
    fn count_list_and_map() {
        assert_eq!(
            call(
                FuncOp::Count,
                vec![Entity::List(vec![
                    Entity::Int(1),
                    Entity::Int(2),
                    Entity::Int(3)
                ])]
            )
            .unwrap(),
            Entity::Int(3)
        );
        let mut m = HashMap::new();
        m.insert("a".into(), Entity::Int(1));
        m.insert("b".into(), Entity::Int(2));
        assert_eq!(
            call(FuncOp::Count, vec![Entity::Map(m)]).unwrap(),
            Entity::Int(2)
        );
    }

    #[test]
    fn get_keys_and_values_deterministic() {
        let mut m = HashMap::new();
        m.insert("b".into(), Entity::Int(2));
        m.insert("a".into(), Entity::Int(1));
        let keys = call(FuncOp::GetKeys, vec![Entity::Map(m.clone())]).unwrap();
        assert_eq!(
            keys,
            Entity::List(vec![Entity::String("a".into()), Entity::String("b".into()),])
        );
        let values = call(FuncOp::GetValues, vec![Entity::Map(m)]).unwrap();
        assert_eq!(values, Entity::List(vec![Entity::Int(1), Entity::Int(2)]));
    }

    #[test]
    fn arity_guard_rejects_wrong_argc() {
        let err = call(FuncOp::Add, vec![Entity::Int(1)]).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::ArgumentError { .. }));
    }
}
