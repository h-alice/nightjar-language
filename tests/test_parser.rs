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

//! Phase 1 integration tests

use nightjar_lang::{parse, NightjarLanguageError};

#[test]
fn accepts_simple_verifier() {
    // `(GT 1 2)` — parses successfully (valid sentence).
    parse("(GT 1 2)").expect("should parse");
}

#[test]
fn rejects_verifier_arity_mismatch() {
    // `(GT 1 2 3)` — produces ParseError / ArityError (arity mismatch).
    let err = parse("(GT 1 2 3)").unwrap_err();
    assert!(
        matches!(
            err,
            NightjarLanguageError::ArgumentError { .. } | NightjarLanguageError::ParseError { .. }
        ),
        "got {:?}",
        err
    );
}

#[test]
fn accepts_nested_connective_with_verifiers() {
    // `(AND (GT 1 0) (LT 1 10))` — connective with nested verifiers.
    parse("(AND (GT 1 0) (LT 1 10))").expect("should parse");
}

#[test]
fn accepts_forall_with_partial_verifier() {
    // `(ForAll (GT 0) .ids)` — quantifier with partial verifier predicate.
    parse("(ForAll (GT 0) .ids)").expect("should parse");
}

#[test]
fn accepts_exists_with_nonempty_predicate() {
    // `(Exists NonEmpty .names)` — quantifier with unary check predicate.
    parse("(Exists NonEmpty .names)").expect("should parse");
}

#[test]
fn rejects_missing_parens_for_multi_arg_operator() {
    // `GT 1 2` — produces ParseError (missing parentheses).
    let err = parse("GT 1 2").unwrap_err();
    assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
}

// ── Additional Phase 1 behaviour checks ────────────────────────

#[test]
fn accepts_root_symbol() {
    parse("(NonEmpty .)").expect("should parse");
}

#[test]
fn accepts_not_expression() {
    parse("(NOT (EQ .status \"inactive\"))").expect("should parse");
}

#[test]
fn accepts_nested_arithmetic_in_verifier() {
    parse("(EQ (Add (Mul 2 3) (Sub 10 4)) 12)").expect("should parse");
}

#[test]
fn accepts_negative_literals() {
    parse("(GT -5 -10)").expect("should parse");
}

#[test]
fn accepts_unicode_symbol() {
    parse("(EQ .數量 100)").expect("should parse");
}

#[test]
fn accepts_unicode_string_literal() {
    parse("(EQ .name \"紅嘴黑鵯\")").expect("should parse");
}

#[test]
fn top_level_bool_literal_parses() {
    parse("True").expect("should parse");
    parse("False").expect("should parse");
}

#[test]
fn combined_examples() {
    // A handful of examples should all parse cleanly.
    for src in [
        "(GE .revenue 0)",
        "(NonEmpty .)",
        "(EQ (Add .dept1_revenue .dept2_revenue) .total_revenue)",
        "(AND (GE .revenue 0) (LT .revenue 1000000))",
        "(NOT (EQ .status \"inactive\"))",
        "(ForAll (GT 0) .scores)",
        "(Exists NonEmpty .names)",
        "(ForAll (GE 0) (GetValues .revenue_by_dept))",
        "(AND (ForAll (GT 0) .scores) (GT (Count .scores) 3))",
    ] {
        parse(src).unwrap_or_else(|e| panic!("failed to parse `{}`: {:?}", src, e));
    }
}
