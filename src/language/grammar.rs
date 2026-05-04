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

//! # Grammar module
//!
//! Defines the grammar, operator enums, token types, and the AST shared
//! by the parser and the executor.
//!
//! NOTE: the AST is designed to have two "tiers": (BoolExpr / ValueExpr).
//!
//! # Formal grammar definition (EBNF)
//!
//! ```ebnf
//! (* A program is a single expression that must reduce to Boolean. *)
//! program         = bool_expr ;
//!
//! bool_expr       = bool_literal
//!                 | verifier_expr
//!                 | connective_expr
//!                 | not_expr
//!                 | unary_check_expr
//!                 | quantifier_expr ;
//!
//! verifier_expr   = "(" verifier_op value_expr value_expr ")" ;
//! verifier_op     = "EQ" | "NE" | "LT" | "LE" | "GT" | "GE" ;
//!
//! connective_expr = "(" connective_op bool_expr bool_expr ")" ;
//! connective_op   = "AND" | "OR" ;
//! not_expr        = "(" "NOT" bool_expr ")" ;
//!
//! unary_check_expr = "(" "NonEmpty" value_expr ")" ;
//!
//! (* Quantifiers: assert a predicate over a List-typed Entity. *)
//! quantifier_expr = "(" quantifier_op predicate value_expr ")" ;
//! quantifier_op   = "ForAll" | "Exists" ;
//!
//! (* Predicates are only legal inside quantifiers.                  *)
//! (* Partial vs. full predicate is disambiguated by operand count   *)
//! (* at parse time, not by a syntactic marker:                      *)
//! (*   (VerifierOp x)       — partial verifier (1 operand)          *)
//! (*   (VerifierOp x y)     — full bool_expr (2 operands)           *)
//! (*   NonEmpty (bare)      — unary check                           *)
//! (*   any other bool_expr  — full bool_expr                        *)
//! (* The body of a full predicate may use the "@" element-rooted    *)
//! (* symbol form to refer to the current iteration element.         *)
//! predicate       = partial_verifier | "NonEmpty" | bool_expr ;
//! partial_verifier = "(" verifier_op value_expr ")" ;
//!
//! (* Value expressions produce an Entity. *)
//! value_expr      = literal
//!                 | symbol
//!                 | func_expr ;
//!
//! (* Arity is enforced at parse time from FuncOp::expected_arity():  *)
//! (*   1-ary: Neg, Abs, Length, Upper, Lower,                        *)
//! (*          Head, Tail, Count, GetKeys, GetValues                  *)
//! (*   2-ary: Add, Sub, Mul, Div, Mod, Concat, Get                   *)
//! (*   3-ary: Substring                                              *)
//! func_expr       = "(" func_op value_expr { value_expr } ")" ;
//! func_op         = arith_op | string_op | collection_op ;
//! arith_op        = "Add" | "Sub" | "Mul" | "Div"
//!                 | "Mod" | "Neg" | "Abs" ;
//! string_op       = "Concat" | "Length" | "Substring"
//!                 | "Upper" | "Lower" ;
//! collection_op   = "Head" | "Tail" | "Get" | "Count"
//!                 | "GetKeys" | "GetValues" ;
//!
//! (* Terminals. *)
//! literal         = int_literal | float_literal
//!                 | string_literal | bool_literal | null_literal ;
//!
//! (* A leading "-" is part of the numeric literal    *)
//! int_literal     = [ "-" ] digit { digit } ;
//! float_literal   = [ "-" ] digit { digit } "." digit { digit } ;
//!
//! string_literal  = '"' { any_char } '"' ;
//! bool_literal    = "True" | "False" ;
//! null_literal    = "Null" ;
//!
//! (* Symbols have two namespaces:                                    *)
//! (*   "." — root-rooted (resolved against the whole input).         *)
//! (*   "@" — element-rooted (resolved against the current iteration  *)
//! (*         element of the nearest enclosing ForAll/Exists).        *)
//! (* Bare "." is the whole input; bare "@" is the current element.   *)
//! (* "@" is only legal inside a quantifier predicate; the parser     *)
//! (* rejects it elsewhere with a ParseError.                         *)
//! symbol          = ( "." | "@" ) [ segment { "." segment } ] ;
//!
//! (* Segment characters are Unicode-aware:                           *)
//! (* char::is_alphanumeric() covers Unicode categories L* and N*,    *)
//! (* so keys like ".營收" and ".données.résultat" are valid.         *)
//! segment         = ident_start { ident_char } ;
//! ident_start     = unicode_letter | "_" ;
//! ident_char      = unicode_letter | unicode_digit | "_" ;
//!
//! digit           = "0" | "1" | "2" | "3" | "4"
//!                 | "5" | "6" | "7" | "8" | "9" ;
//! ```
//!
//! ### Other implementation notes
//!
//! not about grammar itself, but related
//!
//! - Parser enforces a configurable nesting-depth limit
//!   (`ParserConfig::max_depth`, default 256) to protect the host from
//!   stack overflow on adversarial input.
//! - Every AST node carries a byte-offset [`Span`] from Phase 1 so runtime
//!   diagnostics can point into the source string.

use crate::error::Span;

/// Spanned wrapper
///
/// Every AST node and token is wrapped in `Spanned<T>` so source positions
/// are preserved through parsing and into runtime errors.
#[derive(Debug, Clone, PartialEq)]
pub struct Spanned<T> {
    /// The wrapped AST node or token value.
    pub node: T,
    /// Source span of the node within the original expression.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Construct a [`Spanned`] from a node and its source [`Span`].
    pub const fn new(node: T, span: Span) -> Self {
        Self { node, span }
    }
}

/// Token types
///
/// Basic units of the language.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// Left parenthesis, (
    LParen,
    /// Right parenthesis, )
    RParen,
    /// Keyword
    Keyword(Keyword),
    /// Integer literal
    IntLiteral(i64),
    /// Float literal
    FloatLiteral(f64),
    /// String literal
    StringLiteral(String),
    /// Boolean literal
    BoolLiteral(bool),
    /// Null literal
    NullLiteral,
    /// A symbol path.
    ///
    /// `path` holds the segments joined by `.` *without* the leading sigil
    /// i.e. bare root/element has an empty path.
    ///
    /// `root` records which namespace (`.` or `@`) the token was written in.
    Symbol {
        /// The symbol namespace (`.` or `@`).
        root: SymbolRoot,
        /// Dot-joined segment path **without** the leading sigil.
        path: String,
    },
}

/// Discriminator for which namespace a symbol path is rooted in.
///
/// - `Root` — resolved against the whole input (the `.` sigil).
/// - `Element` — resolved against the current iteration element of the
///   nearest enclosing `ForAll`/`Exists` (the `@` sigil).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolRoot {
    /// Root-rooted symbol (`.`), resolved against the whole input.
    Root,
    /// Element-rooted symbol (`@`), resolved against the current iteration
    /// element of the nearest enclosing quantifier.
    Element,
}

/// Keywords
///
/// All reserved keywords (operators, special names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    /// Equality verifier `EQ`.
    EQ,
    /// Inequality verifier `NE`.
    NE,
    /// Less-than verifier `LT`.
    LT,
    /// Less-than-or-equal verifier `LE`.
    LE,
    /// Greater-than verifier `GT`.
    GT,
    /// Greater-than-or-equal verifier `GE`.
    GE,
    /// Unary `NonEmpty` check.
    NonEmpty,
    /// Logical conjunction `AND`.
    AND,
    /// Logical disjunction `OR`.
    OR,
    /// Logical negation `NOT`.
    NOT,
    /// Universal quantifier `ForAll`.
    ForAll,
    /// Existential quantifier `Exists`.
    Exists,
    /// Arithmetic addition `Add`.
    Add,
    /// Arithmetic subtraction `Sub`.
    Sub,
    /// Arithmetic multiplication `Mul`.
    Mul,
    /// Arithmetic division `Div`.
    Div,
    /// Arithmetic modulo `Mod`.
    Mod,
    /// Arithmetic negation `Neg`.
    Neg,
    /// Arithmetic absolute value `Abs`.
    Abs,
    /// String concatenation `Concat`.
    Concat,
    /// String length `Length` (Unicode scalar count).
    Length,
    /// String substring `Substring` (char-indexed).
    Substring,
    /// String upper-case conversion `Upper`.
    Upper,
    /// String lower-case conversion `Lower`.
    Lower,
    /// List head accessor `Head`.
    Head,
    /// List tail accessor `Tail`.
    Tail,
    /// List/map element accessor `Get`.
    Get,
    /// Container size accessor `Count`.
    Count,
    /// Map keys accessor `GetKeys`.
    GetKeys,
    /// Map values accessor `GetValues`.
    GetValues,
}

impl Keyword {
    /// Match an identifier string against the reserved keyword table.
    ///
    /// Currently case-sensitive.
    ///
    /// NOTE: Consider case-insensitive matching in the future.
    pub fn from_ident(s: &str) -> Option<Keyword> {
        Some(match s {
            "EQ" => Keyword::EQ,
            "NE" => Keyword::NE,
            "LT" => Keyword::LT,
            "LE" => Keyword::LE,
            "GT" => Keyword::GT,
            "GE" => Keyword::GE,
            "NonEmpty" => Keyword::NonEmpty,
            "AND" => Keyword::AND,
            "OR" => Keyword::OR,
            "NOT" => Keyword::NOT,
            "ForAll" => Keyword::ForAll,
            "Exists" => Keyword::Exists,
            "Add" => Keyword::Add,
            "Sub" => Keyword::Sub,
            "Mul" => Keyword::Mul,
            "Div" => Keyword::Div,
            "Mod" => Keyword::Mod,
            "Neg" => Keyword::Neg,
            "Abs" => Keyword::Abs,
            "Concat" => Keyword::Concat,
            "Length" => Keyword::Length,
            "Substring" => Keyword::Substring,
            "Upper" => Keyword::Upper,
            "Lower" => Keyword::Lower,
            "Head" => Keyword::Head,
            "Tail" => Keyword::Tail,
            "Get" => Keyword::Get,
            "Count" => Keyword::Count,
            "GetKeys" => Keyword::GetKeys,
            "GetValues" => Keyword::GetValues,
            _ => return None,
        })
    }
}

// ╭─────────────────────────────────────────────────────────╮
//  ═══════════════ Operator sub-enums (for AST) ═══════════════
// ╰─────────────────────────────────────────────────────────╯

/// Verifier operators
///
/// Verifiers are binary operators that return a boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifierOp {
    /// Equality.
    EQ,
    /// Inequality.
    NE,
    /// Less-than.
    LT,
    /// Less-than-or-equal.
    LE,
    /// Greater-than.
    GT,
    /// Greater-than-or-equal.
    GE,
}

impl VerifierOp {
    /// Match a keyword against the verifier operator table.
    pub fn from_keyword(kw: Keyword) -> Option<Self> {
        Some(match kw {
            Keyword::EQ => VerifierOp::EQ,
            Keyword::NE => VerifierOp::NE,
            Keyword::LT => VerifierOp::LT,
            Keyword::LE => VerifierOp::LE,
            Keyword::GT => VerifierOp::GT,
            Keyword::GE => VerifierOp::GE,
            _ => return None,
        })
    }
}

/// Unary check operators
///
/// As name suggested, these operators are special verifiers that only take one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryCheckOp {
    /// `NonEmpty` — false for `Null` and empty `String`/`List`/`Map`,
    /// true otherwise.
    NonEmpty,
}

impl UnaryCheckOp {
    /// Match a keyword against the unary check operator table.
    pub fn from_keyword(kw: Keyword) -> Option<Self> {
        Some(match kw {
            Keyword::NonEmpty => UnaryCheckOp::NonEmpty,
            _ => return None,
        })
    }
}

/// Quantifier operators
///
/// These operators are used to quantify over a collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantifierOp {
    /// Universal quantifier — predicate must hold for every element.
    ForAll,
    /// Existential quantifier — predicate must hold for at least one element.
    Exists,
}

impl QuantifierOp {
    /// Match a keyword against the quantifier operator table.
    pub fn from_keyword(kw: Keyword) -> Option<Self> {
        Some(match kw {
            Keyword::ForAll => QuantifierOp::ForAll,
            Keyword::Exists => QuantifierOp::Exists,
            _ => return None,
        })
    }
}

/// Function operators
///
/// These operators are used to perform some operations on values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuncOp {
    /// Arithmetic addition (binary).
    Add,
    /// Arithmetic subtraction (binary).
    Sub,
    /// Arithmetic multiplication (binary).
    Mul,
    /// Arithmetic division (binary); errors on divide-by-zero.
    Div,
    /// Arithmetic modulo (binary); errors on divide-by-zero.
    Mod,
    /// Arithmetic negation (unary).
    Neg,
    /// Arithmetic absolute value (unary).
    Abs,
    /// String concatenation (binary).
    Concat,
    /// String length in Unicode scalars (unary).
    Length,
    /// String substring `(Substring s start len)` (ternary, char-indexed).
    Substring,
    /// String upper-case conversion (unary, Unicode-aware).
    Upper,
    /// String lower-case conversion (unary, Unicode-aware).
    Lower,
    /// First element of a list (unary); errors on empty.
    Head,
    /// All but the first element of a list (unary); errors on empty.
    Tail,
    /// Indexed accessor: `(Get list Int)` or `(Get map String)` (binary).
    Get,
    /// Size of a list or map (unary).
    Count,
    /// Sorted list of map keys (unary).
    GetKeys,
    /// Map values sorted by key (unary).
    GetValues,
}

impl FuncOp {
    /// Match a keyword against the function operator table.
    pub fn from_keyword(kw: Keyword) -> Option<Self> {
        Some(match kw {
            Keyword::Add => FuncOp::Add,
            Keyword::Sub => FuncOp::Sub,
            Keyword::Mul => FuncOp::Mul,
            Keyword::Div => FuncOp::Div,
            Keyword::Mod => FuncOp::Mod,
            Keyword::Neg => FuncOp::Neg,
            Keyword::Abs => FuncOp::Abs,
            Keyword::Concat => FuncOp::Concat,
            Keyword::Length => FuncOp::Length,
            Keyword::Substring => FuncOp::Substring,
            Keyword::Upper => FuncOp::Upper,
            Keyword::Lower => FuncOp::Lower,
            Keyword::Head => FuncOp::Head,
            Keyword::Tail => FuncOp::Tail,
            Keyword::Get => FuncOp::Get,
            Keyword::Count => FuncOp::Count,
            Keyword::GetKeys => FuncOp::GetKeys,
            Keyword::GetValues => FuncOp::GetValues,
            _ => return None,
        })
    }

    /// Return the expected arity of the function operator.
    pub fn expected_arity(&self) -> usize {
        match self {
            FuncOp::Neg
            | FuncOp::Abs
            | FuncOp::Length
            | FuncOp::Upper
            | FuncOp::Lower
            | FuncOp::Head
            | FuncOp::Tail
            | FuncOp::Count
            | FuncOp::GetKeys
            | FuncOp::GetValues => 1,
            FuncOp::Add
            | FuncOp::Sub
            | FuncOp::Mul
            | FuncOp::Div
            | FuncOp::Mod
            | FuncOp::Concat
            | FuncOp::Get => 2,
            FuncOp::Substring => 3,
        }
    }

    /// Return the name of the function operator.
    pub fn name(&self) -> &'static str {
        match self {
            FuncOp::Add => "Add",
            FuncOp::Sub => "Sub",
            FuncOp::Mul => "Mul",
            FuncOp::Div => "Div",
            FuncOp::Mod => "Mod",
            FuncOp::Neg => "Neg",
            FuncOp::Abs => "Abs",
            FuncOp::Concat => "Concat",
            FuncOp::Length => "Length",
            FuncOp::Substring => "Substring",
            FuncOp::Upper => "Upper",
            FuncOp::Lower => "Lower",
            FuncOp::Head => "Head",
            FuncOp::Tail => "Tail",
            FuncOp::Get => "Get",
            FuncOp::Count => "Count",
            FuncOp::GetKeys => "GetKeys",
            FuncOp::GetValues => "GetValues",
        }
    }
}

/// Literals
///
/// "Words", the building blocks of the language. (for example, "apple",
/// 123, 45.6, true, false, null)
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    /// 64-bit signed integer literal.
    Int(i64),
    /// IEEE-754 double-precision float literal.
    Float(f64),
    /// UTF-8 string literal.
    String(String),
    /// Boolean literal (`True` / `False`).
    Bool(bool),
    /// `Null` literal.
    Null,
}

// ╭────────────────────────────────────────────────╮
//  ═════════════════ AST (two-tier) ═════════════════
// ╰────────────────────────────────────────────────╯

/// The AST type which produces booleans.
pub type SpannedBoolExpr = Spanned<BoolExpr>;

/// The AST type which produces values.
pub type SpannedValueExpr = Spanned<ValueExpr>;

/// Top-level program, must evaluate to Boolean.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Root boolean expression of the program.
    pub expr: SpannedBoolExpr,
}

/// Boolean-producing expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum BoolExpr {
    /// Boolean literal `True` or `False`.
    Literal(bool),

    /// Verifier takes two values, evaluates, and returns a boolean.
    Verifier {
        /// Verifier operator.
        op: VerifierOp,
        /// Left-hand value expression.
        left: Box<SpannedValueExpr>,
        /// Right-hand value expression.
        right: Box<SpannedValueExpr>,
    },

    /// Logical AND.
    And(Box<SpannedBoolExpr>, Box<SpannedBoolExpr>),

    /// Logical OR.
    Or(Box<SpannedBoolExpr>, Box<SpannedBoolExpr>),

    /// Logical NOT.
    Not(Box<SpannedBoolExpr>),

    /// Boolean-producing checks which only take one operand.
    UnaryCheck {
        /// Unary check operator.
        op: UnaryCheckOp,
        /// Operand value expression.
        operand: Box<SpannedValueExpr>,
    },

    /// Quantifier takes a predicate and an operand, and returns a boolean.
    Quantifier {
        /// Quantifier operator.
        op: QuantifierOp,
        /// Predicate applied to each element.
        predicate: Spanned<Predicate>,
        /// Collection-producing operand.
        operand: Box<SpannedValueExpr>,
    },
}

/// Value-producing expressions (Entity).
///
/// There are three kinds of value-producing expressions:
/// - Literals (immediate values): `123`, `45.6`, `"hello"`, `true`, `false`, `null`.
/// - Symbols (references to Symbol table): `@.a`, `@.b`, `@.c`.
/// - Results from function calls: `(Add 1 2)`, `(Sub 1 2)`.
#[derive(Debug, Clone, PartialEq)]
pub enum ValueExpr {
    /// Immediate literal value.
    Literal(Literal),
    /// A symbol reference.
    ///
    /// `path` is the dot-joined segment string without
    /// the leading sigil (empty string = bare root/element).
    ///
    /// `root` tells the executor which namespace to resolve against.
    Symbol {
        /// The root type (namespace, *local* `@` or *global* `.``)
        root: SymbolRoot,
        /// The dot-joined path segments (empty string = bare root/element).
        path: String,
    },
    /// Function-call expression that reduces to a value.
    FuncCall {
        /// Function operator being invoked.
        op: FuncOp,
        /// Argument value expressions.
        args: Vec<SpannedValueExpr>,
    },
}

/// Predicate, unary boolean function, **used exclusively in quantifiers**.
///
/// You can think of a predicate as *curried* verifier.
///
/// `PartialVerifier` and `UnaryCheck` are the ergonomic fast paths whose
/// bound value is evaluated once at setup time.
///
/// `Full` carries a general boolean expression that is re-evaluated per
/// iteration element with the element bound in scope — this is what enables
/// `(ForAll (EQ @.a @.b) …)`.
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    /// Partially-fulfilled verifier (one argument known).
    ///
    /// For example, `(EQ @.a)`, notice that the second argument is missing.
    PartialVerifier {
        /// Verifier operator.
        op: VerifierOp,
        /// Pre-supplied operand; the iteration element fills the missing slot.
        bound: Box<SpannedValueExpr>,
    },
    /// Unary check, used in quantifiers.
    ///
    /// For example, `(NonEmpty @.a)`.
    UnaryCheck(UnaryCheckOp),
    /// Full boolean expression, used in quantifiers.
    ///
    /// For example, `(EQ @.a @.b)`.
    Full(Box<SpannedBoolExpr>),
}

/// Tests!
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_from_ident_matches_reserved_words() {
        assert_eq!(Keyword::from_ident("EQ"), Some(Keyword::EQ));
        assert_eq!(Keyword::from_ident("NonEmpty"), Some(Keyword::NonEmpty));
        assert_eq!(Keyword::from_ident("ForAll"), Some(Keyword::ForAll));
        assert_eq!(Keyword::from_ident("GetValues"), Some(Keyword::GetValues));
    }

    #[test]
    fn keyword_from_ident_rejects_unknown_and_is_case_sensitive() {
        assert_eq!(Keyword::from_ident("eq"), None);
        assert_eq!(Keyword::from_ident("foobar"), None);
        assert_eq!(Keyword::from_ident(""), None);
    }

    #[test]
    fn verifier_op_conversion_covers_all_variants() {
        let pairs = [
            (Keyword::EQ, VerifierOp::EQ),
            (Keyword::NE, VerifierOp::NE),
            (Keyword::LT, VerifierOp::LT),
            (Keyword::LE, VerifierOp::LE),
            (Keyword::GT, VerifierOp::GT),
            (Keyword::GE, VerifierOp::GE),
        ];
        for (kw, op) in pairs {
            assert_eq!(VerifierOp::from_keyword(kw), Some(op));
        }
        assert_eq!(VerifierOp::from_keyword(Keyword::AND), None);
    }

    #[test]
    fn quantifier_op_conversion() {
        assert_eq!(
            QuantifierOp::from_keyword(Keyword::ForAll),
            Some(QuantifierOp::ForAll)
        );
        assert_eq!(
            QuantifierOp::from_keyword(Keyword::Exists),
            Some(QuantifierOp::Exists)
        );
        assert_eq!(QuantifierOp::from_keyword(Keyword::NonEmpty), None);
    }

    #[test]
    fn func_op_arity_table_matches_spec() {
        let one = [
            FuncOp::Neg,
            FuncOp::Abs,
            FuncOp::Length,
            FuncOp::Upper,
            FuncOp::Lower,
            FuncOp::Head,
            FuncOp::Tail,
            FuncOp::Count,
            FuncOp::GetKeys,
            FuncOp::GetValues,
        ];
        let two = [
            FuncOp::Add,
            FuncOp::Sub,
            FuncOp::Mul,
            FuncOp::Div,
            FuncOp::Mod,
            FuncOp::Concat,
            FuncOp::Get,
        ];
        let three = [FuncOp::Substring];

        for op in one {
            assert_eq!(op.expected_arity(), 1, "{:?} should have arity 1", op);
        }
        for op in two {
            assert_eq!(op.expected_arity(), 2, "{:?} should have arity 2", op);
        }
        for op in three {
            assert_eq!(op.expected_arity(), 3, "{:?} should have arity 3", op);
        }
    }

    #[test]
    fn func_op_from_keyword_rejects_non_func_keywords() {
        assert_eq!(FuncOp::from_keyword(Keyword::AND), None);
        assert_eq!(FuncOp::from_keyword(Keyword::EQ), None);
        assert_eq!(FuncOp::from_keyword(Keyword::ForAll), None);
        assert_eq!(FuncOp::from_keyword(Keyword::Add), Some(FuncOp::Add));
    }

    #[test]
    fn spanned_wrapper_constructs_and_exposes_fields() {
        let sp = Spanned::new(Literal::Int(42), Span::new(0, 2));
        assert_eq!(sp.node, Literal::Int(42));
        assert_eq!(sp.span, Span::new(0, 2));
    }

    #[test]
    fn ast_structural_equality_sanity() {
        // Build `(GT (Add 1 2) 0)` as an AST and assert round-trip equality.
        let left = Spanned::new(
            ValueExpr::FuncCall {
                op: FuncOp::Add,
                args: vec![
                    Spanned::new(ValueExpr::Literal(Literal::Int(1)), Span::new(5, 6)),
                    Spanned::new(ValueExpr::Literal(Literal::Int(2)), Span::new(7, 8)),
                ],
            },
            Span::new(1, 9),
        );
        let right = Spanned::new(ValueExpr::Literal(Literal::Int(0)), Span::new(10, 11));
        let expr = Spanned::new(
            BoolExpr::Verifier {
                op: VerifierOp::GT,
                left: Box::new(left.clone()),
                right: Box::new(right.clone()),
            },
            Span::new(0, 12),
        );
        let program = Program { expr: expr.clone() };
        assert_eq!(program.expr, expr);
    }

    #[test]
    fn func_op_name_round_trips_through_keyword() {
        // A quick consistency check: for every FuncOp we can recover its keyword.
        for op in [
            FuncOp::Add,
            FuncOp::Sub,
            FuncOp::Mul,
            FuncOp::Div,
            FuncOp::Mod,
            FuncOp::Neg,
            FuncOp::Abs,
            FuncOp::Concat,
            FuncOp::Length,
            FuncOp::Substring,
            FuncOp::Upper,
            FuncOp::Lower,
            FuncOp::Head,
            FuncOp::Tail,
            FuncOp::Get,
            FuncOp::Count,
            FuncOp::GetKeys,
            FuncOp::GetValues,
        ] {
            let kw = Keyword::from_ident(op.name()).expect("name should be a keyword");
            assert_eq!(FuncOp::from_keyword(kw), Some(op));
        }
    }
}
