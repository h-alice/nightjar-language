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

//! Integration tests.

#![cfg(feature = "json")]

use nightjar_lang::{exec, ExecOptions, ExecResult, NightjarLanguageError};
use serde_json::json;

fn run(expression: &str, data: serde_json::Value) -> ExecResult {
    exec(expression, data, ExecOptions::default())
}

#[test]
fn gt_1_2_is_false() {
    assert_eq!(run("(GT 1 2)", json!({})), ExecResult::False);
}

#[test]
fn eq_1_1_is_true() {
    assert_eq!(run("(EQ 1 1)", json!({})), ExecResult::True);
}

#[test]
fn gt_gt_1_is_error() {
    // `GT GT 1` — `GT` is not a value expression.
    let r = run("(GT GT 1)", json!({}));
    assert!(matches!(r, ExecResult::Error(_)), "got {:?}", r);
}

#[test]
fn ge_revenue_100_against_100() {
    assert_eq!(
        run("(GE .revenue 100)", json!({ "revenue": 100 })),
        ExecResult::True
    );
}

#[test]
fn eq_add_depts_vs_total() {
    assert_eq!(
        run(
            "(ForAll (EQ (Add @.dept1 @.dept2) @.total) . )",
            json!([ { "dept1": 100, "dept2": 200, "total": 300 } ])
        ),
        ExecResult::True
    );
}

#[test]
fn connective_revenue_and_nonempty_name() {
    assert_eq!(
        run(
            "(AND (GE .revenue 0) (NonEmpty .name))",
            json!({ "revenue": 50, "name": "Acme" })
        ),
        ExecResult::True
    );
}

#[test]
fn forall_scores_positive_true() {
    assert_eq!(
        run("(ForAll (GT 0) .scores)", json!({ "scores": [1, 2, 3] })),
        ExecResult::True
    );
}

#[test]
fn forall_scores_positive_false() {
    assert_eq!(
        run("(ForAll (GT 0) .scores)", json!({ "scores": [0, 1, 2] })),
        ExecResult::False
    );
}

#[test]
fn exists_admin_role() {
    assert_eq!(
        run(
            "(Exists (EQ \"admin\") .roles)",
            json!({ "roles": ["user", "admin"] })
        ),
        ExecResult::True
    );
}

#[test]
fn forall_scalar_fallback() {
    assert_eq!(
        run("(ForAll (GT 0) .count)", json!({ "count": 5 })),
        ExecResult::True
    );
}

#[test]
fn forall_on_map_is_type_error() {
    let r = run("(ForAll (GT 0) .data)", json!({ "data": { "a": 1 } }));
    assert!(
        matches!(
            r,
            ExecResult::Error(NightjarLanguageError::TypeError { .. })
        ),
        "got {:?}",
        r
    );
}

// ── Extra coverage ─────────────────────────────────────────────

#[test]
fn epsilon_equality_for_float_sum() {
    assert_eq!(run("(EQ (Add 0.1 0.2) 0.3)", json!({})), ExecResult::True);
}

#[test]
fn not_inactive_status_is_true_when_active() {
    assert_eq!(
        run(
            "(NOT (EQ .status \"inactive\"))",
            json!({ "status": "active" })
        ),
        ExecResult::True
    );
}

#[test]
fn count_of_scores_gt_3() {
    assert_eq!(
        run(
            "(AND (ForAll (GT 0) .scores) (GT (Count .scores) 3))",
            json!({ "scores": [1, 2, 3, 4] })
        ),
        ExecResult::True
    );
}

#[test]
fn getvalues_on_map_then_forall() {
    assert_eq!(
        run(
            "(ForAll (GE 0) (GetValues .revenue_by_dept))",
            json!({ "revenue_by_dept": { "a": 10, "b": 20 } })
        ),
        ExecResult::True
    );
}

#[test]
fn list_element_addressable_via_underscore_index() {
    assert_eq!(
        run("(GT .ids._1 15)", json!({ "ids": [10, 20, 30] })),
        ExecResult::True
    );
}

#[test]
fn nonempty_on_root_symbol() {
    assert_eq!(run("(NonEmpty .)", json!({ "x": 1 })), ExecResult::True);
    assert_eq!(run("(NonEmpty .)", json!({})), ExecResult::False);
}

#[test]
fn missing_symbol_is_error() {
    let r = run("(GT .absent 0)", json!({}));
    assert!(matches!(
        r,
        ExecResult::Error(NightjarLanguageError::SymbolNotFound { .. })
    ));
}

#[test]
fn integer_overflow_is_error() {
    let r = run("(EQ (Add 9223372036854775807 1) 0)", json!({}));
    assert!(matches!(
        r,
        ExecResult::Error(NightjarLanguageError::IntegerOverflow { .. })
    ));
}

#[test]
fn division_by_zero_is_error() {
    let r = run("(EQ (Div 1 0) 0)", json!({}));
    assert!(matches!(
        r,
        ExecResult::Error(NightjarLanguageError::DivisionByZero { .. })
    ));
}

// ── Unicode end-to-end ─────────────────────────────────────────

#[test]
fn unicode_symbol_resolution_cjk() {
    assert_eq!(
        run("(GE .營收 0)", json!({ "營收": 500 })),
        ExecResult::True
    );
}

#[test]
fn unicode_symbol_resolution_latin_with_diacritics() {
    assert_eq!(
        run(
            "(EQ .données.résultat \"oui\")",
            json!({ "données": { "résultat": "oui" } })
        ),
        ExecResult::True
    );
}

#[test]
fn length_counts_unicode_chars_not_bytes() {
    assert_eq!(
        run("(EQ (Length .name) 2)", json!({ "name": "營收" })),
        ExecResult::True
    );
}
