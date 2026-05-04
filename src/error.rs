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

//! # Error module
//!
//! Unified error types, stable error codes, and source-position spans used
//! for diagnostics across both the parser and the runtime executor.

use thiserror::Error;

/// Byte-offset span in the source expression
/// used **exclusively for error tracking and diagnostics**.
///
/// `Span` carries source positions so that errors can point back at the
/// offending token, and offers richer error reporting.
///
/// ## Note
/// Offsets are in **bytes** (not char indices), compatible with Rust's
/// native `&str` slicing, and safe across Unicode content because the
/// tokenizer only records positions at char boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl Span {
    /// Construct a span covering `[start, end)` in byte offsets.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// Construct a zero-width span at a single byte offset, useful for
    /// errors at EOF or between tokens.
    pub const fn point(at: usize) -> Self {
        Self { start: at, end: at }
    }
}

/// Stable, machine-readable error codes.
///
/// Quick reference:
/// - E001: ParseError
/// - E002: TypeError
/// - E003: ArityError
/// - E004: SymbolNotFound
/// - E005: AmbiguousSymbol
/// - E006: DivisionByZero
/// - E007: RecursionError
/// - E008: IndexError
/// - E009: IntegerOverflow
/// - E010: ScopeError
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// Parse error: expression does not conform to the grammar.
    E001,
    /// Type error:operator applied to incompatible types.
    E002,
    /// Argument error: wrong operand count for an operator.
    E003,
    /// Symbol not found: symbol path is not present in the payload.
    E004,
    /// Ambiguous symbol: reserved for a future shorthand-lookup mode.
    E005,
    /// Division by zero in `Div` or `Mod`.
    E006,
    /// Recursion error: AST nesting exceeded `max_depth`.
    E007,
    /// Index error: `Get`/`Head`/`Tail` ran off the end of a list.
    E008,
    /// Integer overflow during checked arithmetic.
    E009,
    /// Scope error: `@` symbol used outside any quantifier predicate.
    E010,
}

/// Unified error type for the entire crate.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum NightjarLanguageError {
    /// Expression does not conform to the grammar (E001).
    #[error("[{code:?}] Parse error at {span:?}: {message}")]
    ParseError {
        /// Source span of the offending token.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// Operator applied to incompatible types (E002).
    #[error("[{code:?}] Type error at {span:?}: {message}")]
    TypeError {
        /// Source span of the offending expression.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// Wrong number of operands for an operator (E003).
    #[error("[{code:?}] Argument error at {span:?}: {message}")]
    ArgumentError {
        /// Source span of the offending call.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// Symbol path not present in the payload (E004).
    #[error("[{code:?}] Symbol not found at {span:?}: {message}")]
    SymbolNotFound {
        /// Source span of the offending symbol reference.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// Reserved for a future shorthand-lookup mode; not raised today (E005).
    #[error("[{code:?}] Ambiguous symbol at {span:?}: {message}")]
    AmbiguousSymbol {
        /// Source span of the offending symbol reference.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// `Div` or `Mod` invoked with a zero divisor (E006).
    #[error("[{code:?}] Division by zero at {span:?}: {message}")]
    DivisionByZero {
        /// Source span of the offending operation.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// AST nesting exceeded the configured `max_depth` (E007).
    #[error("[{code:?}] Recursion depth limit exceeded at {span:?}: {message}")]
    RecursionError {
        /// Source span at which the limit was exceeded.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// `Get`/`Head`/`Tail` ran off the end of a list (E008).
    #[error("[{code:?}] Index out of bounds at {span:?}: {message}")]
    IndexError {
        /// Source span of the offending operation.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// Checked integer arithmetic overflowed (E009).
    #[error("[{code:?}] Integer overflow at {span:?}: {message}")]
    IntegerOverflow {
        /// Source span of the offending operation.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },

    /// `@` element-relative symbol used outside any quantifier predicate (E010).
    #[error("[{code:?}] Scope error at {span:?}: {message}")]
    ScopeError {
        /// Source span of the offending symbol reference.
        span: Span,
        /// Stable error code.
        code: ErrorCode,
        /// Human-readable diagnostic.
        message: String,
    },
}

impl NightjarLanguageError {
    /// Source span of the error within the original expression text.
    pub fn span(&self) -> Span {
        match self {
            NightjarLanguageError::ParseError { span, .. }
            | NightjarLanguageError::TypeError { span, .. }
            | NightjarLanguageError::ArgumentError { span, .. }
            | NightjarLanguageError::SymbolNotFound { span, .. }
            | NightjarLanguageError::AmbiguousSymbol { span, .. }
            | NightjarLanguageError::DivisionByZero { span, .. }
            | NightjarLanguageError::RecursionError { span, .. }
            | NightjarLanguageError::IndexError { span, .. }
            | NightjarLanguageError::IntegerOverflow { span, .. }
            | NightjarLanguageError::ScopeError { span, .. } => *span,
        }
    }

    /// Stable [`ErrorCode`] tag identifying the error variant.
    pub fn code(&self) -> ErrorCode {
        match self {
            NightjarLanguageError::ParseError { code, .. }
            | NightjarLanguageError::TypeError { code, .. }
            | NightjarLanguageError::ArgumentError { code, .. }
            | NightjarLanguageError::SymbolNotFound { code, .. }
            | NightjarLanguageError::AmbiguousSymbol { code, .. }
            | NightjarLanguageError::DivisionByZero { code, .. }
            | NightjarLanguageError::RecursionError { code, .. }
            | NightjarLanguageError::IndexError { code, .. }
            | NightjarLanguageError::IntegerOverflow { code, .. }
            | NightjarLanguageError::ScopeError { code, .. } => *code,
        }
    }

    /// Human-readable diagnostic message attached to the error.
    pub fn message(&self) -> &str {
        match self {
            NightjarLanguageError::ParseError { message, .. }
            | NightjarLanguageError::TypeError { message, .. }
            | NightjarLanguageError::ArgumentError { message, .. }
            | NightjarLanguageError::SymbolNotFound { message, .. }
            | NightjarLanguageError::AmbiguousSymbol { message, .. }
            | NightjarLanguageError::DivisionByZero { message, .. }
            | NightjarLanguageError::RecursionError { message, .. }
            | NightjarLanguageError::IndexError { message, .. }
            | NightjarLanguageError::IntegerOverflow { message, .. }
            | NightjarLanguageError::ScopeError { message, .. } => message,
        }
    }
}

// ╭──────────────────────────────────────────────────────────────────╮
//  ═══════════════════ Internal helper constructors ════════════════════
// ╰──────────────────────────────────────────────────────────────────╯
//
// These are `pub(crate)` so every module can produce spanned errors in a
// convenient way.

/// Build a `ParseError` (code `E001`).
///
/// Used by the tokenizer and the recursive-descent parser when the input
/// fails a lexical or grammatical rule (unexpected character, missing `)`,
/// stray token after expression, etc.).
///
/// # Example
///
/// ```ignore
/// use crate::error::{parse_error, ErrorCode, Span};
/// let err = parse_error(Span::new(4, 5), "unexpected token");
/// assert_eq!(err.code(), ErrorCode::E001);
/// assert!(err.message().contains("unexpected"));
/// ```
pub(crate) fn parse_error(span: Span, message: impl Into<String>) -> NightjarLanguageError {
    NightjarLanguageError::ParseError {
        span,
        code: ErrorCode::E001,
        message: message.into(),
    }
}

/// Build a `TypeError` (code `E002`).
///
/// Raised by the executor when operands flow into a function or verifier with
/// an incompatible runtime type — for example `(GT "a" 1)` or quantifying
/// over a `Map`.
///
/// # Example
///
/// ```ignore
/// use crate::error::{type_error, ErrorCode, Span};
/// let err = type_error(Span::new(0, 8), "cannot compare String with Int");
/// assert_eq!(err.code(), ErrorCode::E002);
/// ```
pub(crate) fn type_error(span: Span, message: impl Into<String>) -> NightjarLanguageError {
    NightjarLanguageError::TypeError {
        span,
        code: ErrorCode::E002,
        message: message.into(),
    }
}

/// Build an `ArgumentError` (code `E003`).
///
/// Raised by the parser when a fixed-arity operator gets the wrong number of
/// operands, e.g. `(GT 1 2 3)`.
///
/// # Example
///
/// ```ignore
/// use crate::error::{argument_error, ErrorCode, Span};
/// let err = argument_error(Span::new(7, 8), "verifier takes exactly 2 operands");
/// assert_eq!(err.code(), ErrorCode::E003);
/// ```
pub(crate) fn argument_error(span: Span, message: impl Into<String>) -> NightjarLanguageError {
    NightjarLanguageError::ArgumentError {
        span,
        code: ErrorCode::E003,
        message: message.into(),
    }
}

/// Build a `SymbolNotFound` error (code `E004`).
///
/// Raised at runtime when a symbol path does not exist in the symbol table,
/// or when an element-relative `@` path misses a field on the current
/// iteration element.
///
/// # Example
///
/// ```ignore
/// use crate::error::{symbol_not_found, ErrorCode, Span};
/// let err = symbol_not_found(Span::new(4, 12), ".data.missing");
/// assert_eq!(err.code(), ErrorCode::E004);
/// assert!(err.message().contains(".data.missing"));
/// ```
pub(crate) fn symbol_not_found(span: Span, path: &str) -> NightjarLanguageError {
    NightjarLanguageError::SymbolNotFound {
        span,
        code: ErrorCode::E004,
        message: format!("symbol `{}` not found", path),
    }
}

/// Build a `DivisionByZero` error (code `E006`).
///
/// Raised by `Div` / `Mod` when the divisor reduces to `0` (Int) or `0.0` (Float).
///
/// # Example
///
/// ```ignore
/// use crate::error::{division_by_zero, ErrorCode, Span};
/// let err = division_by_zero(Span::new(5, 12));
/// assert_eq!(err.code(), ErrorCode::E006);
/// ```
pub(crate) fn division_by_zero(span: Span) -> NightjarLanguageError {
    NightjarLanguageError::DivisionByZero {
        span,
        code: ErrorCode::E006,
        message: "division or modulo by zero".to_string(),
    }
}

/// Build a `RecursionError` error (code `E007`).
///
/// Raised by the parser when the depth of an expression exceeds
/// `ParserConfig::max_depth`, guards the host against stack overflow from
/// adversarial input.
///
/// # Example
///
/// ```ignore
/// use crate::error::{recursion_error, ErrorCode, Span};
/// let err = recursion_error(Span::new(0, 1), 256);
/// assert_eq!(err.code(), ErrorCode::E007);
/// assert!(err.message().contains("256"));
/// ```
pub(crate) fn recursion_error(span: Span, limit: usize) -> NightjarLanguageError {
    NightjarLanguageError::RecursionError {
        span,
        code: ErrorCode::E007,
        message: format!("expression recursion depth exceeds limit ({})", limit),
    }
}

/// Build an `IndexError` error (code `E008`).
///
/// Raised by the `Get` function when a list index lies outside `0..len`.
///
/// # Example
///
/// ```ignore
/// use crate::error::{index_error, ErrorCode, Span};
/// let err = index_error(Span::new(5, 12), 7, 3);
/// assert_eq!(err.code(), ErrorCode::E008);
/// assert!(err.message().contains('7') && err.message().contains('3'));
/// ```
pub(crate) fn index_error(span: Span, idx: i64, len: usize) -> NightjarLanguageError {
    NightjarLanguageError::IndexError {
        span,
        code: ErrorCode::E008,
        message: format!("index {} out of bounds for list of length {}", idx, len),
    }
}

/// Build an `IntegerOverflow` error (code `E009`).
///
/// Raised by arithmetic functions when a `checked_*` operation on `i64`
/// overflows (e.g. `Add` of `i64::MAX + 1`, `Neg` of `i64::MIN`).
///
/// # Example
///
/// ```ignore
/// use crate::error::{integer_overflow, ErrorCode, Span};
/// let err = integer_overflow(Span::new(0, 10), "Add");
/// assert_eq!(err.code(), ErrorCode::E009);
/// assert!(err.message().contains("Add"));
/// ```
pub(crate) fn integer_overflow(span: Span, op: &str) -> NightjarLanguageError {
    NightjarLanguageError::IntegerOverflow {
        span,
        code: ErrorCode::E009,
        message: format!("integer overflow in {}", op),
    }
}

/// Build a `ScopeError` (code `E010`). Raised by the post-parse validator
/// (or, as a defensive fallback, by the executor) when an element-relative
/// `@` symbol appears outside any enclosing `ForAll` / `Exists` predicate.
///
/// # Example
///
/// ```ignore
/// use crate::error::{scope_error, ErrorCode, Span};
/// let err = scope_error(Span::new(4, 6), "`@` used outside a quantifier");
/// assert_eq!(err.code(), ErrorCode::E010);
/// ```
pub(crate) fn scope_error(span: Span, message: impl Into<String>) -> NightjarLanguageError {
    NightjarLanguageError::ScopeError {
        span,
        code: ErrorCode::E010,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_constructors() {
        assert_eq!(Span::new(3, 7), Span { start: 3, end: 7 });
        assert_eq!(Span::point(5), Span { start: 5, end: 5 });
    }

    #[test]
    fn span_accessor_roundtrip() {
        let span = Span::new(10, 20);
        let err = parse_error(span, "bad token");
        assert_eq!(err.span(), span);
    }

    #[test]
    fn code_accessor_per_variant() {
        assert_eq!(parse_error(Span::point(0), "x").code(), ErrorCode::E001);
        assert_eq!(type_error(Span::point(0), "x").code(), ErrorCode::E002);
        assert_eq!(argument_error(Span::point(0), "x").code(), ErrorCode::E003);
        assert_eq!(
            symbol_not_found(Span::point(0), ".foo").code(),
            ErrorCode::E004
        );
        assert_eq!(division_by_zero(Span::point(0)).code(), ErrorCode::E006);
        assert_eq!(recursion_error(Span::point(0), 256).code(), ErrorCode::E007);
        assert_eq!(index_error(Span::point(0), 5, 3).code(), ErrorCode::E008);
        assert_eq!(
            integer_overflow(Span::point(0), "Add").code(),
            ErrorCode::E009
        );
        assert_eq!(
            scope_error(Span::point(0), "@ outside quantifier").code(),
            ErrorCode::E010
        );
    }

    #[test]
    fn message_accessor() {
        let err = type_error(Span::new(0, 3), "bad types");
        assert_eq!(err.message(), "bad types");
    }

    #[test]
    fn display_formatting_contains_code_and_span() {
        let err = parse_error(Span::new(4, 9), "unexpected token");
        let rendered = format!("{}", err);
        assert!(rendered.contains("E001"));
        assert!(rendered.contains("4"));
        assert!(rendered.contains("9"));
        assert!(rendered.contains("unexpected token"));
    }

    #[test]
    fn symbol_not_found_formats_path() {
        let err = symbol_not_found(Span::point(0), ".data.missing");
        assert!(err.message().contains(".data.missing"));
    }

    #[test]
    fn index_out_of_bounds_formats_idx_and_len() {
        let err = index_error(Span::point(0), 7, 3);
        assert!(err.message().contains('7'));
        assert!(err.message().contains('3'));
    }
}
