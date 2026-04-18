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

//! # Parser module
//!
//! This module includes tokenizer and recursive-descent parser that turn
//! a Nightjar source string into a spanned AST, with Unicode-safe byte
//! offsets and configurable nesting-depth limits.

use crate::error::{
    argument_error, parse_error, recursion_error, scope_error, NightjarLanguageError, Span,
};
use crate::language::grammar::{
    BoolExpr, FuncOp, Keyword, Literal, Predicate, Program, QuantifierOp, Spanned, SpannedBoolExpr,
    SpannedValueExpr, SymbolRoot, Token, UnaryCheckOp, ValueExpr, VerifierOp,
};

/// Configuration for the parser.
#[derive(Debug, Clone)]
pub struct ParserConfig {
    /// Max depth of AST. Default is 256.
    ///
    /// If the AST depth exceeds this value, a `RecursionError` will be
    /// returned.
    ///
    /// (TBH, who will verify data with that many operators?)
    pub max_depth: usize,
}

impl Default for ParserConfig {
    fn default() -> Self {
        Self { max_depth: 256 }
    }
}

/// Tokenizer
///
/// Tokenizer turns a Nightjar language source string into a stream of tokens.
pub struct Tokenizer<'a> {
    /// The program we want to parse.
    input: &'a str,
    /// Index into `chars` for the next un-consumed char.
    cursor: usize,
    /// Character vector + byte offsets. We need byte offsets for Span, and
    /// char-level access so that Unicode content inside strings/symbols is
    /// handled without slicing through a codepoint.
    chars: Vec<(usize, char)>,
    /// End-of-input byte offset (i.e. `input.len()`).
    eof: usize,
}

impl<'a> Tokenizer<'a> {
    /// Initialize a tokenizer for `input`.
    ///
    /// Build a tokenizer for `input`, precomputing `(byte_offset, char)`
    /// pairs so peeking and slicing stay cheap and Unicode-safe.
    pub fn new(input: &'a str) -> Self {
        let chars: Vec<(usize, char)> = input.char_indices().collect();
        Self {
            input,
            cursor: 0,
            chars,
            eof: input.len(),
        }
    }

    /// Main tokenizing procedure.
    ///
    /// Consume the whole input and return the full token stream with spans,
    /// or a `ParseError` on the first unrecoverable lexical issue.
    pub fn tokenize(&mut self) -> Result<Vec<Spanned<Token>>, NightjarLanguageError> {
        let mut tokens = Vec::new();
        loop {
            // Loop until EOF.
            self.skip_whitespace(); // Skip encountered whitespace.
            if self.is_eof() {
                // Exit if EOF.
                break;
            }
            let start = self.byte_pos(); // Record the next-unconsumed byte offset.
            let c = self.peek_char().expect("Runtime error, unexpected EOF"); // NOTE: This should never happen.
            let token = match c {
                '(' => {
                    self.advance();
                    Token::LParen
                }
                ')' => {
                    self.advance();
                    Token::RParen
                }
                '"' => self.read_string(start)?,
                '.' => self.read_symbol(start, SymbolRoot::Root)?,
                '@' => self.read_symbol(start, SymbolRoot::Element)?,
                '-' if self.is_negative_literal() => self.read_number(start)?,
                c if c.is_ascii_digit() => self.read_number(start)?,
                c if c.is_alphabetic() || c == '_' => self.read_ident(start)?,
                other => {
                    return Err(parse_error(
                        Span::new(start, start + other.len_utf8()),
                        format!("unexpected character `{}`", other),
                    ));
                }
            };
            let end = self.byte_pos();
            tokens.push(Spanned::new(token, Span::new(start, end)));
        }
        Ok(tokens)
    }

    // ────────────────────── position helpers ──────────────────────

    /// Get byte-level offset
    ///
    /// Byte offset of the next un-consumed character, or EOF offset if the
    /// cursor has run off the end. Used for building `Span`s.
    fn byte_pos(&self) -> usize {
        if self.cursor < self.chars.len() {
            self.chars[self.cursor].0
        } else {
            self.eof
        }
    }

    /// `true` when no more characters remain to tokenize.
    fn is_eof(&self) -> bool {
        self.cursor >= self.chars.len()
    }

    /// Get next character without consuming it.
    ///
    /// Return the next character without consuming it, or `None` at EOF.
    fn peek_char(&self) -> Option<char> {
        self.chars.get(self.cursor).map(|(_, c)| *c)
    }

    /// Get character with given offset
    ///
    /// Return the character at `cursor + offset` without consuming it.
    /// Useful for two-char lookahead, e.g. distinguishing `-5` from `- `.
    fn peek_char_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.cursor + offset).map(|(_, c)| *c)
    }

    /// Consume one character by moving the cursor forward.
    fn advance(&mut self) {
        self.cursor += 1;
    }

    /// Consume any run of whitespace characters so the next `peek_char`
    /// returns the start of the next token (or EOF).
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Negative literal checker
    ///
    /// If met a `-` character, this function will check if the next character
    /// is a digit.
    ///
    /// If so, it will return `true`, otherwise `false`.
    fn is_negative_literal(&self) -> bool {
        self.peek_char() == Some('-') && self.peek_char_at(1).map_or(false, |c| c.is_ascii_digit())
        // If the character is not alphabetic, it can't be a digit.
    }

    // ────────────────────────── readers ──────────────────────────

    /// String reader
    ///
    /// Consume a `"..."` string literal. The opening `"` has not yet been
    /// consumed. Errors if EOF is reached before a closing `"`.
    ///
    /// While using `read_string`, the position MUST point at the start of the string literal
    /// opening, that is, the first `"`.
    fn read_string(&mut self, start: usize) -> Result<Token, NightjarLanguageError> {
        // consume opening "
        self.advance();
        let mut buf = String::new();
        loop {
            match self.peek_char() {
                Some('"') => {
                    self.advance();
                    return Ok(Token::StringLiteral(buf));
                }
                Some(c) => {
                    buf.push(c); // Push read content into buffer.
                    self.advance();
                }
                None => {
                    return Err(parse_error(
                        Span::new(start, self.byte_pos()),
                        "unterminated string literal",
                    ));
                }
            }
        }
    }

    /// Number reader
    ///
    /// Consume an integer or floating-point literal starting at `start`.
    /// Produces `Token::IntLiteral` by default, and `Token::FloatLiteral`
    /// when a `.digit` fractional part is present.
    fn read_number(&mut self, start: usize) -> Result<Token, NightjarLanguageError> {
        // Handle optional leading '-'
        if self.peek_char() == Some('-') {
            self.advance();
        }
        // Consume all digits until meet non-digit character.
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.advance();
            } else {
                break;
            }
        }

        // Optional fractional part, only if '.' followed by at least one digit.
        let mut is_float = false; // A flag indicating whether the number is a float.
        if self.peek_char() == Some('.') && // Current char is '.'
            self.peek_char_at(1)            // Next char is a digit
                .map_or(false, |c| c.is_ascii_digit())
        {
            // If next char is a digit, then it is a float.
            is_float = true;
            self.advance(); // '.'
                            // Consume all subsequent digits as the fractional part.
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        // If everything works fine, now we've consumed all the digits and the decimal point.
        let end = self.byte_pos(); // Get the end position of the number.
        let text = &self.input[start..end]; // Get the number as a string from input.
        if is_float {
            // Check the flag if it is a float.
            text.parse::<f64>().map(Token::FloatLiteral).map_err(|_| {
                // Try to parse the number as a float.
                parse_error(
                    // If not, return an error.
                    Span::new(start, end),
                    format!("invalid float literal `{}`", text),
                )
            }) // Return successfully parsed float or error.
        } else {
            // If not a float, try to parse it as an integer.
            text.parse::<i64>().map(Token::IntLiteral).map_err(|_| {
                // Try to parse the number as an integer.
                parse_error(
                    // If not, return an error.
                    Span::new(start, end),
                    format!("invalid integer literal `{}`", text),
                )
            }) // Return successfully parsed integer or error.
        }
    }

    /// Symbol reader
    ///
    /// The leading sigil (`.` for root, `@` for the current iteration element)
    /// has already been peeked but not consumed.
    ///
    /// Accepts:
    /// - the bare sigil alone (`.` or `@`) when followed by whitespace, `)`,
    ///   or EOF — returns an empty `path`,
    /// - dotted paths such as `.data.department_1.revenue` or `@._0.a`.
    ///
    /// Explanation of the difference between `.` and `@`:
    /// - `.` doubles as both the root marker **and** the separator before its
    ///   first segment (so `.a` is read as sigil + `a`).
    /// - `@` is a standalone marker that requires an explicit `.` separator
    ///   before any segments (so `@.a` is sigil + `.` + `a`; `@a` is rejected).
    ///
    /// The returned `Token::Symbol` stores `path` *without* any leading
    /// sigil or separator; `root` records which sigil was used.
    fn read_symbol(
        &mut self,
        _start: usize,
        root: SymbolRoot, // Leading sigil ('@' or '.')
    ) -> Result<Token, NightjarLanguageError> {
        self.advance(); // consume the leading sigil ('.' or '@')
        let sigil: char = match root {
            SymbolRoot::Root => '.',    // Root sigil is '.'
            SymbolRoot::Element => '@', // Element sigil is '@'
        };
        let mut path = String::new(); // Initialize path of the symbol

        // Root's '.' sigil also serves as the separator for the first
        // segment, so we attempt to read a segment immediately. For Element,
        // the first segment (if any) must be introduced by an explicit '.'.
        match root {
            // The root is `.`, try to read the first segment
            SymbolRoot::Root => {
                match self.try_read_segment() {
                    // Try to read the first segment
                    Some(seg) => Self::push_segment(&mut path, seg), // There's some segment after `.`
                    None => return self.complete_bare_sigil(root, path, sigil),
                }
            }
            // If the root is `@` and the next character is not `.`, then it is a bare sigil.
            SymbolRoot::Element if self.peek_char() != Some('.') => {
                return self.complete_bare_sigil(root, path, sigil); // Complete the token, it is a `@`
            }
            _ => {}
        }

        // Consume any number of `.segment` continuations.
        while self.peek_char() == Some('.') {
            let dot_pos = self.byte_pos();
            self.advance(); // consume '.'
            match self.try_read_segment() {
                Some(seg) => Self::push_segment(&mut path, seg),
                None => {
                    return Err(parse_error(
                        Span::new(dot_pos, dot_pos + 1),
                        "expected symbol segment after `.`",
                    ));
                }
            }
        }
        Ok(Token::Symbol { root, path })
    }

    /// Try to read a symbol segment
    ///
    /// Try to consume one identifier segment (Unicode alphanumeric or `_`).
    /// Returns the segment as a `&str` slice, or `None` if no segment
    /// characters were present at the current position.
    ///
    /// The caller is responsible for assembling segments into a dot-joined
    /// path (see [`push_segment`]).
    ///
    /// Example (internal; tokenizer-private):
    ///
    /// ```ignore
    /// let mut tz = Tokenizer::new("foo.bar");
    /// assert_eq!(tz.try_read_segment(), Some("foo"));
    /// // Caller is responsible for consuming the `.` separator itself
    /// // before the next call.
    /// ```
    fn try_read_segment(&mut self) -> Option<&str> {
        let seg_start = self.byte_pos(); // Current position of the parser
        while let Some(c) = self.peek_char() {
            // Advance the parser until a non-alphanumeric character is encountered
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let seg_end = self.byte_pos(); // Current position of the parser
        if seg_start == seg_end {
            // Empty segment
            None
        } else {
            Some(&self.input[seg_start..seg_end]) // Return the segment as a &str slice
        }
    }

    /// Push symbol segment
    ///
    /// Append a segment to a dot-separated path string. Inserts a `.`
    /// separator when `path` is non-empty.
    fn push_segment(path: &mut String, seg: &str) {
        if !path.is_empty() {
            path.push('.');
        }
        path.push_str(seg);
    }

    /// Complete sigil reading and construct the tokenized symbol.
    ///
    /// The function has the following purposes:
    /// - Complete tokenization of a bare sigil (`.` or `@` with no path
    ///   segments).
    /// - Validates that the next character is a legal terminator
    ///   (whitespace, `)`, or EOF).
    /// - Produces the final `Token::Symbol` instance.
    /// - Returns a parse error if an unexpected character follows.
    ///
    /// Example (internal):
    ///
    /// ```ignore
    /// // `.` followed by EOF → bare root symbol.
    /// let tz = Tokenizer::new(".");
    /// let tok = tz.complete_bare_sigil(SymbolRoot::Root, String::new(), '.').unwrap();
    /// assert!(matches!(tok, Token::Symbol { path, .. } if path.is_empty()));
    /// ```
    fn complete_bare_sigil(
        &self,
        root: SymbolRoot, // Root sigil ('@' or '.')
        path: String,     // Path of the symbol
        sigil: char,      // Sigil
    ) -> Result<Token, NightjarLanguageError> {
        match self.peek_char() {
            None => Ok(Token::Symbol { root, path }), // Next char is EOF, complete the token
            Some(c) if c.is_whitespace() || c == ')' =>
            // Next char is vaild terminator, complete the token
            {
                Ok(Token::Symbol { root, path })
            }
            Some(c) => {
                let pos = self.byte_pos();
                Err(parse_error(
                    Span::new(pos, pos + c.len_utf8()),
                    format!("unexpected character `{}` after `{}`", c, sigil),
                ))
            }
        }
    }

    /// Identifier reader
    ///
    /// Consume an alphanumeric identifier and classify it as `True`,
    /// `False`, `Null`, or a known operator keyword.
    ///
    /// Unknown identifiers become `ParseError`, we do not support user
    /// defined variable names (for now).
    fn read_ident(&mut self, start: usize) -> Result<Token, NightjarLanguageError> {
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        let end = self.byte_pos();
        let text = &self.input[start..end];
        match text {
            "True" => Ok(Token::BoolLiteral(true)),
            "False" => Ok(Token::BoolLiteral(false)),
            "Null" => Ok(Token::NullLiteral),
            _ => match Keyword::from_ident(text) {
                Some(kw) => Ok(Token::Keyword(kw)),
                None => Err(parse_error(
                    Span::new(start, end),
                    format!("unknown identifier `{}`", text),
                )),
            },
        }
    }

    // ────────────────────────────────────────────────────────────
}

/// Parser
///
/// Parser turns a token stream into a spanned AST.
pub struct Parser {
    tokens: Vec<Spanned<Token>>,
    pos: usize,
    depth: usize,
    max_depth: usize,
    input_len: usize,
}

impl Parser {
    /// Run the parser over a full token stream and return a `Program`
    /// whose top-level expression is guaranteed (by grammar) to reduce
    /// to a boolean. Fails on leftover tokens after the expression.
    pub fn parse(
        tokens: Vec<Spanned<Token>>,
        config: &ParserConfig,
    ) -> Result<Program, NightjarLanguageError> {
        let input_len = tokens.last().map(|t| t.span.end).unwrap_or(0);
        let mut p = Self {
            tokens,
            pos: 0,
            depth: 0,
            max_depth: config.max_depth,
            input_len,
        };
        let expr = p.parse_bool_expr()?;
        p.expect_eof()?;
        Ok(Program { expr })
    }

    // ───────────────────── helpers for parsing ──────────────────────

    /// Look at the current token (with its span) without consuming it.
    fn peek(&self) -> Option<&Spanned<Token>> {
        self.tokens.get(self.pos)
    }

    /// Look at the current token kind without consuming it
    ///
    /// Convenience wrapper for when the span is not needed.
    fn peek_token(&self) -> Option<&Token> {
        self.peek().map(|t| &t.node)
    }

    /// Advance the parser by one token and return a clone of the token
    /// we just consumed (including its span).
    ///
    /// Callers use this to pull a token they have already validated via `peek`.
    fn bump(&mut self) -> Spanned<Token> {
        let t = self.tokens[self.pos].clone();
        self.pos += 1;
        t
    }

    /// Span of the current token, or a zero-width point-span at EOF.
    ///
    /// Preferred over `peek().span` for producing error spans because
    /// it gracefully handles the end-of-input case.
    fn current_span(&self) -> Span {
        self.peek()
            .map(|t| t.span)
            .unwrap_or(Span::point(self.input_len)) // Points to the end of the input
    }

    /// Consume a `)` or issue parse error.
    fn expect_rparen(&mut self) -> Result<Span, NightjarLanguageError> {
        match self.peek_token() {
            Some(Token::RParen) => Ok(self.bump().span),
            _ => Err(parse_error(self.current_span(), "expected `)`")),
        }
    }

    /// Ensure the token stream is fully consumed.
    ///
    /// Any leftover token is reported as a parse error.
    /// We designed Nightjar programs to be single expressions.
    fn expect_eof(&mut self) -> Result<(), NightjarLanguageError> {
        match self.peek() {
            None => Ok(()),
            Some(t) => Err(parse_error(
                t.span,
                "unexpected token after complete expression",
            )),
        }
    }

    /// Called when entering a parenthesized sub-expression.
    ///
    /// Increments the nesting counter and returns `DepthLimitExceeded` if the
    /// configured `max_depth` is reached.
    ///
    /// This prevents stack overflow from pathological inputs like deeply nested `NOT`s.
    fn enter_depth(&mut self, span: Span) -> Result<(), NightjarLanguageError> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(recursion_error(span, self.max_depth));
        }
        Ok(())
    }

    /// Counterpart to `enter_depth`, call on exit from a parenthesized
    /// sub-expression.
    fn exit_depth(&mut self) {
        self.depth = self.depth.saturating_sub(1); // Saturating to avoid any chance of underflow.
    }

    // ─────────────────────────────────────────────────────────

    // ───────────────────── AST producers ──────────────────────

    /// Parse a `bool_expr`
    ///
    /// A `bool_expr` is either a bare `True`/`False` literal or a
    /// parenthesized boolean operator form (verifier, connective,
    /// quantifier, `NOT`, or `NonEmpty`).
    fn parse_bool_expr(&mut self) -> Result<SpannedBoolExpr, NightjarLanguageError> {
        let start_span = self.current_span();
        match self.peek_token() {
            Some(Token::BoolLiteral(b)) => {
                // Boolean literal (true/false)
                let b = *b;
                let span = self.bump().span;
                Ok(Spanned::new(BoolExpr::Literal(b), span))
            }
            Some(Token::LParen) => {
                // Parenthesized boolean expression
                let lparen_span = self.bump().span;
                self.enter_depth(lparen_span)?; // Increment depth
                let result = self.parse_bool_body(lparen_span.start);
                self.exit_depth(); // Decrement depth
                result
            }
            Some(_) => Err(parse_error(start_span, "expected boolean expression")),
            None => Err(parse_error(
                start_span,
                "expected boolean expression, got end of input",
            )),
        }
    }

    /// Parse the body of a parenthesized boolean expression.
    ///
    /// Dispatching on the leading keyword to the appropriate sub-parser.
    ///
    /// Expects the opening `(` has already been consumed. This function is responsible for
    /// matching the closing `)` and producing a span from `start` to `)`.
    fn parse_bool_body(&mut self, start: usize) -> Result<SpannedBoolExpr, NightjarLanguageError> {
        // The '(' has already been consumed.
        let kw = self.expect_keyword_token()?;
        match kw.node { // Match boolean expressions over keywords
            Keyword::EQ | Keyword::NE | Keyword::LT | Keyword::LE | Keyword::GT | Keyword::GE => {
                // All verifiers take exactly two arguments.
                let op = VerifierOp::from_keyword(kw.node).unwrap(); // NOTE: double check if unwrap is safe
                let left = self.parse_value_expr()?;  // Left expression
                let right = self.parse_value_expr()?; // Right expression

                // Use a different rparen parser for verifiers, which produces a better error message
                let close = self.expect_rparen_for_verifier(kw.span)?;
                Ok(Spanned::new(
                    BoolExpr::Verifier {
                        op,
                        left: Box::new(left),
                        right: Box::new(right),
                    },
                    Span::new(start, close.end),
                ))
            }

            Keyword::AND => {
                let l = self.parse_bool_expr()?;
                let r = self.parse_bool_expr()?;
                let close = self.expect_rparen()?;
                Ok(Spanned::new(
                    BoolExpr::And(Box::new(l), Box::new(r)),
                    Span::new(start, close.end),
                ))
            }

            Keyword::OR => {
                let l = self.parse_bool_expr()?;
                let r = self.parse_bool_expr()?;
                let close = self.expect_rparen()?;
                Ok(Spanned::new(
                    BoolExpr::Or(Box::new(l), Box::new(r)),
                    Span::new(start, close.end),
                ))
            }

            Keyword::NOT => {
                let inner = self.parse_bool_expr()?;
                let close = self.expect_rparen()?;
                Ok(Spanned::new(
                    BoolExpr::Not(Box::new(inner)),
                    Span::new(start, close.end),
                ))
            }

            Keyword::NonEmpty => {
                let operand = self.parse_value_expr()?;
                let close = self.expect_rparen()?;
                Ok(Spanned::new(
                    BoolExpr::UnaryCheck {
                        op: UnaryCheckOp::NonEmpty,
                        operand: Box::new(operand),
                    },
                    Span::new(start, close.end),
                ))
            }

            Keyword::ForAll | Keyword::Exists => {
                let op = QuantifierOp::from_keyword(kw.node).unwrap();
                let predicate = self.parse_predicate()?; // Predicates are partial-fulfiled verifiers
                let operand = self.parse_value_expr()?;
                let close = self.expect_rparen()?;
                Ok(Spanned::new(
                    BoolExpr::Quantifier {
                        op,
                        predicate,
                        operand: Box::new(operand),
                    },
                    Span::new(start, close.end),
                ))
            }
            other => Err(parse_error(
                kw.span,
                format!(
                    "expected boolean operator (verifier / connective / quantifier / NonEmpty), found `{:?}`",
                    other
                ),
            )),
        }
    }

    /// Specialized `)` matcher for verifier forms for more verbose error messages.
    ///
    /// If a value-startable token appears where `)` is expected, surface an `ArgumentError`
    /// instead of a generic parse error.
    fn expect_rparen_for_verifier(
        &mut self,
        _kw_span: Span,
    ) -> Result<Span, NightjarLanguageError> {
        match self.peek_token() {
            Some(Token::RParen) => Ok(self.bump().span),
            Some(_) => {
                let sp = self.current_span();
                Err(argument_error(sp, "verifier takes exactly 2 operands"))
            }
            None => Err(parse_error(
                self.current_span(),
                "expected `)` to close verifier",
            )),
        }
    }

    /// Parse a `predicate`.
    ///
    /// Predicates are the first operand of a quantifier.
    ///
    /// Accepts:
    /// 1. the bare unary check `NonEmpty` → `Predicate::UnaryCheck`,
    /// 2. `(VerifierOp x)` (1 operand) → `Predicate::PartialVerifier`,
    /// 3. `(VerifierOp x y)` (2 operands) → `Predicate::Full(Verifier)`,
    /// 4. any other `bool_expr` (connectives, `NOT`, `(NonEmpty …)`, nested
    ///    quantifiers, bool literals) → `Predicate::Full`.
    ///
    /// The partial vs. full split for verifier-headed forms is decided by
    /// operand count at parse time.
    ///
    /// Little note on grammar: A predicate is only the part of a clause
    /// containing the verb and its modifiers, telling what the subject does or
    /// is (e.g., "larger than x"). A clause is a structure, while a predicate is
    /// a functional component.
    fn parse_predicate(&mut self) -> Result<Spanned<Predicate>, NightjarLanguageError> {
        // Case 1: bare `NonEmpty` keyword (no parentheses).
        if matches!(self.peek_token(), Some(Token::Keyword(Keyword::NonEmpty))) {
            let span = self.bump().span;
            return Ok(Spanned::new(
                Predicate::UnaryCheck(UnaryCheckOp::NonEmpty),
                span,
            ));
        }
        // Case 2/3: `(VerifierOp ...)`: partial or full verifier depending
        // on operand count. We peek 2 tokens ahead to detect this shape.
        if matches!(self.peek_token(), Some(Token::LParen))
            && matches!(
                self.tokens.get(self.pos + 1).map(|t| &t.node),
                Some(Token::Keyword(
                    Keyword::EQ
                        | Keyword::NE
                        | Keyword::LT
                        | Keyword::LE
                        | Keyword::GT
                        | Keyword::GE
                )),
            )
        {
            let lparen_span = self.bump().span;
            self.enter_depth(lparen_span)?;
            let result = self.parse_verifier_predicate(lparen_span.start);
            self.exit_depth();
            return result;
        }
        // Case 4 — any other bool_expr: delegate and wrap as Full.
        let body = self.parse_bool_expr()?;
        let span = body.span;
        Ok(Spanned::new(Predicate::Full(Box::new(body)), span))
    }

    /// Parse a verifier-headed predicate.
    ///
    /// Expects the opening `(` has already been consumed. Decides
    /// `PartialVerifier` vs `Full(Verifier)` by how many value operands appear
    /// before the closing `)`.
    ///
    /// Note that the no-argument verifier `(NonEmpty)` is handled in
    /// `parse_predicate`.
    fn parse_verifier_predicate(
        &mut self,
        start: usize,
    ) -> Result<Spanned<Predicate>, NightjarLanguageError> {
        let kw = self.expect_keyword_token()?;
        let op = VerifierOp::from_keyword(kw.node).ok_or_else(|| {
            parse_error(
                kw.span,
                "verifier predicate must use a verifier operator (EQ/NE/LT/LE/GT/GE)",
            )
        })?;
        let first = self.parse_value_expr()?; // We have at least one operand
        match self.peek_token() {
            Some(Token::RParen) => {
                // Complete with partial verifier
                let close = self.bump().span;
                Ok(Spanned::new(
                    Predicate::PartialVerifier {
                        op,
                        bound: Box::new(first),
                    },
                    Span::new(start, close.end),
                ))
            }
            Some(_) => {
                // Still have one more operand, complete with full verifier
                let second = self.parse_value_expr()?;
                let close = self.expect_rparen_for_verifier(kw.span)?; // At most 2 operands
                let body_span = Span::new(start, close.end);
                let body = Spanned::new(
                    BoolExpr::Verifier {
                        op,
                        left: Box::new(first),
                        right: Box::new(second),
                    },
                    body_span,
                );
                Ok(Spanned::new(Predicate::Full(Box::new(body)), body_span))
            }
            None => Err(parse_error(
                self.current_span(),
                "expected `)` or value expression in verifier predicate",
            )),
        }
    }

    /// Parse a value expression.
    ///
    /// Accepts:
    /// - a literal
    /// - a symbol path
    /// - a nested `(func_op ...)` call that produces an Entity.
    fn parse_value_expr(&mut self) -> Result<SpannedValueExpr, NightjarLanguageError> {
        let start_span = self.current_span();
        match self.peek_token() {
            // Case 1: literal
            Some(Token::IntLiteral(_))
            | Some(Token::FloatLiteral(_))
            | Some(Token::StringLiteral(_))
            | Some(Token::BoolLiteral(_))
            | Some(Token::NullLiteral) => {
                let tok = self.bump();
                let lit = match tok.node {
                    // Convert literal token to Literal enum, it's lit.
                    Token::IntLiteral(i) => Literal::Int(i),
                    Token::FloatLiteral(f) => Literal::Float(f),
                    Token::StringLiteral(s) => Literal::String(s),
                    Token::BoolLiteral(b) => Literal::Bool(b),
                    Token::NullLiteral => Literal::Null,
                    _ => unreachable!(),
                };
                Ok(Spanned::new(ValueExpr::Literal(lit), tok.span))
            }

            // Case 2: symbol path
            Some(Token::Symbol { .. }) => {
                let tok = self.bump();
                let (root, path) = match tok.node {
                    Token::Symbol { root, path } => (root, path),
                    _ => unreachable!(),
                };
                Ok(Spanned::new(ValueExpr::Symbol { root, path }, tok.span))
            }

            // Case 3: function call, starts with `(`
            Some(Token::LParen) => {
                let lparen_span = self.bump().span;
                self.enter_depth(lparen_span)?;
                let result = self.parse_func_call(lparen_span.start);
                self.exit_depth();
                result
            }

            // Error cases
            Some(_) => Err(parse_error(start_span, "expected value expression")),
            None => Err(parse_error(
                start_span,
                "expected value expression, got end of input",
            )),
        }
    }

    /// Parse a parenthesized function call `(func_op arg ...)`.
    ///
    /// Expects the opening `(` has already been consumed.
    ///
    /// Argument count is validated against the operator's fixed arity, too few
    /// is a generic parse error (missing operand), too many is an explicit
    /// `ArgumentError`.
    fn parse_func_call(&mut self, start: usize) -> Result<SpannedValueExpr, NightjarLanguageError> {
        let kw = self.expect_keyword_token()?;
        let op = FuncOp::from_keyword(kw.node).ok_or_else(|| {
            parse_error(
                kw.span,
                format!(
                    "`{:?}` is not a value-producing function in this position",
                    kw.node
                ),
            )
        })?;
        let expected_operand_count = op.expected_arity();
        let mut args = Vec::with_capacity(expected_operand_count);

        // Attempt to recursively parse an operand
        for _ in 0..expected_operand_count {
            args.push(self.parse_value_expr()?);
        }

        // After consuming exactly `expected` args, require `)`. Any extra token
        // before `)` means the caller supplied too many arguments.
        let close = match self.peek_token() {
            Some(Token::RParen) => self.bump().span, // consume the closing `)`
            Some(_) => {
                return Err(argument_error(
                    self.current_span(),
                    format!(
                        "`{}` takes exactly {} argument(s)",
                        op.name(),
                        expected_operand_count
                    ),
                ));
            }
            None => {
                return Err(parse_error(
                    self.current_span(),
                    format!("expected `)` to close `{}` call", op.name()),
                ));
            }
        };
        Ok(Spanned::new(
            ValueExpr::FuncCall { op, args },
            Span::new(start, close.end),
        ))
    }

    /// Consume the next token only if it is a `Keyword`.
    ///
    /// Returns the keyword and its span, or a parse error if the token is anything
    /// else (or we are at EOF).
    fn expect_keyword_token(&mut self) -> Result<Spanned<Keyword>, NightjarLanguageError> {
        match self.peek_token() {
            Some(Token::Keyword(_)) => {
                let tok = self.bump();
                if let Token::Keyword(kw) = tok.node {
                    Ok(Spanned::new(kw, tok.span))
                } else {
                    unreachable!()
                }
            }
            _ => Err(parse_error(
                self.current_span(),
                "expected operator keyword",
            )),
        }
    }
}
// ─────────────────────────────────────────────────────────

// ──────────────── Convenience entry points ─────────────────

/// Tokenize and parse `input` with default parser configuration.
pub fn parse(input: &str) -> Result<Program, NightjarLanguageError> {
    parse_with_config(input, &ParserConfig::default())
}

/// Tokenize and parse `input` with the supplied `ParserConfig`.
///
/// Primarily useful for tuning `max_depth` in tests or embedded use.
pub fn parse_with_config(
    input: &str,
    config: &ParserConfig,
) -> Result<Program, NightjarLanguageError> {
    let tokens = Tokenizer::new(input).tokenize()?;
    let program = Parser::parse(tokens, config)?;
    validate_scope(&program)?;
    Ok(program)
}
// ─────────────────────────────────────────────────────────

// ───────────────── Syntax scope validator ──────────────────

// Static check that every `@` element-rooted symbol sits inside the
// `predicate` sub-tree of some enclosing `ForAll`/`Exists`.
//
// The operand position of a quantifier is NOT counted as a predicate context,
// a list being quantified is itself resolved against root, not the element.

/// Entry point for the post-parse scope check.
///
/// Walks the whole `Program` with an initial predicate-depth of `0`, so any
/// `@` at the outermost level fails immediately.
///
/// Example (internal):
///
/// ```ignore
/// // `(EQ @.a 1)` at top level: no enclosing quantifier → ScopeError.
/// use crate::language::parser::validate_scope;
/// // ... parse the program here, then:
/// // assert!(matches!(validate_scope(&program), Err(NightjarError::ScopeError{..})));
/// ```
fn validate_scope(program: &Program) -> Result<(), NightjarLanguageError> {
    walk_bool(&program.expr, 0)
}

/// Walk a `BoolExpr`, forwarding the current `predicate_depth` into
/// every sub-expression.
///
/// Entering a quantifier's predicate increments the counter; the quantifier's
/// operand keeps the same depth because the list being iterated is still
/// resolved against the outer scope.
///
/// ## Note
///
/// `predicate_depth` tracks how many quantifier-predicate scopes enclose
/// the current position:
///
/// - `0`: outside all predicates; `@` symbols are illegal here.
/// - `1`: inside one `ForAll`/`Exists` predicate; `@` resolves to its
///   iteration element.
/// - `> 1`: nested quantifiers; `@` resolves to the innermost element.
///
/// Example (internal):
///
/// ```ignore
/// // Inside this call, any `ValueExpr::Symbol { root: Element, .. }`
/// // with `predicate_depth == 0` will produce a ScopeError.
/// walk_bool(&program.expr, 0)?;
/// ```
fn walk_bool(expr: &SpannedBoolExpr, predicate_depth: u32) -> Result<(), NightjarLanguageError> {
    match &expr.node {
        BoolExpr::Literal(_) => Ok(()),
        BoolExpr::Verifier { left, right, .. } => {
            walk_value(left, predicate_depth)?;
            walk_value(right, predicate_depth)
        }
        BoolExpr::And(l, r) | BoolExpr::Or(l, r) => {
            walk_bool(l, predicate_depth)?;
            walk_bool(r, predicate_depth)
        }
        BoolExpr::Not(inner) => walk_bool(inner, predicate_depth),
        BoolExpr::UnaryCheck { operand, .. } => walk_value(operand, predicate_depth),
        BoolExpr::Quantifier {
            predicate, operand, ..
        } => {
            walk_predicate(predicate, predicate_depth + 1)?;
            // The quantifier's operand (the list being iterated) is resolved
            // against the current scope, not the predicate's inner scope
            // so keep the same depth here.
            //
            // (ForAll (EQ @.a @.b) .items)
            //  ^ same depth        ^ same depth
            walk_value(operand, predicate_depth)
        }
    }
}

/// Walk a `ValueExpr`.
///
/// This is the only place that actually reports a
/// `ScopeError`: when a `Symbol` with `root == Element` is seen at
/// `predicate_depth == 0`, we know it sits outside every enclosing
/// quantifier predicate and therefore has no element to resolve against.
///
/// Example (internal):
///
/// ```ignore
/// // Element symbols are only legal when predicate_depth > 0.
/// walk_value(&operand, 0)?;                           // root `.`: always ok
/// walk_value(&inner_at_symbol, /* inside pred */ 1)?; // `@.a`: ok
/// ```
fn walk_value(expr: &SpannedValueExpr, predicate_depth: u32) -> Result<(), NightjarLanguageError> {
    match &expr.node {
        ValueExpr::Literal(_) => Ok(()),
        ValueExpr::Symbol { root, .. } => {
            if matches!(root, SymbolRoot::Element) && predicate_depth == 0 {
                Err(scope_error(
                    expr.span,
                    "`@` element-relative symbols may only appear inside a ForAll/Exists predicate",
                ))
            } else {
                Ok(())
            }
        }
        ValueExpr::FuncCall { args, .. } => {
            for a in args {
                walk_value(a, predicate_depth)?;
            }
            Ok(())
        }
    }
}

/// Walk the bound operand of a partial verifier, or the body of a
/// `Full` predicate.
///
/// With `predicate_depth` already incremented by the caller, `@` is legal
/// anywhere inside either.
///
/// Example (internal):
///
/// ```ignore
/// // `(ForAll (EQ @.a @.b) .items)`: walk_predicate is called with
/// // predicate_depth = 1, which permits `@.a` / `@.b` under Predicate::Full.
/// walk_predicate(&predicate, 1)?;
/// ```
fn walk_predicate(
    pred: &Spanned<Predicate>,
    predicate_depth: u32,
) -> Result<(), NightjarLanguageError> {
    match &pred.node {
        Predicate::PartialVerifier { bound, .. } => walk_value(bound, predicate_depth),
        Predicate::UnaryCheck(_) => Ok(()),
        Predicate::Full(body) => walk_bool(body, predicate_depth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Tokenizer tests ──────────────────────────────────────

    fn tokenize(input: &str) -> Vec<Token> {
        Tokenizer::new(input)
            .tokenize()
            .expect("tokenization should succeed")
            .into_iter()
            .map(|s| s.node)
            .collect()
    }

    #[test]
    fn tokenizes_parentheses_and_keywords() {
        let toks = tokenize("(EQ 1 1)");
        assert_eq!(
            toks,
            vec![
                Token::LParen,
                Token::Keyword(Keyword::EQ),
                Token::IntLiteral(1),
                Token::IntLiteral(1),
                Token::RParen,
            ]
        );
    }

    #[test]
    fn tokenizes_negative_integer_literal() {
        let toks = tokenize("-5");
        assert_eq!(toks, vec![Token::IntLiteral(-5)]);
    }

    #[test]
    fn tokenizes_negative_float_literal() {
        let toks = tokenize("-3.14");
        assert_eq!(toks, vec![Token::FloatLiteral(-3.14)]);
    }

    #[test]
    fn space_between_minus_and_digit_is_error() {
        let err = Tokenizer::new("- 5").tokenize().unwrap_err();
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn tokenizes_string_literal_with_unicode() {
        let toks = tokenize("\"營收\"");
        assert_eq!(toks, vec![Token::StringLiteral("營收".into())]);
    }

    #[test]
    fn tokenizes_unterminated_string_errors() {
        let err = Tokenizer::new("\"abc").tokenize().unwrap_err();
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn tokenizes_root_symbol_bare_dot() {
        assert_eq!(
            tokenize("."),
            vec![Token::Symbol {
                root: SymbolRoot::Root,
                path: "".into()
            }]
        );
        assert_eq!(
            tokenize("(NonEmpty .)"),
            vec![
                Token::LParen,
                Token::Keyword(Keyword::NonEmpty),
                Token::Symbol {
                    root: SymbolRoot::Root,
                    path: "".into()
                },
                Token::RParen,
            ]
        );
    }

    #[test]
    fn tokenizes_nested_symbol_path() {
        assert_eq!(
            tokenize(".data.department_1.revenue"),
            vec![Token::Symbol {
                root: SymbolRoot::Root,
                path: "data.department_1.revenue".into()
            }]
        );
    }

    #[test]
    fn tokenizes_unicode_symbol() {
        assert_eq!(
            tokenize(".營收"),
            vec![Token::Symbol {
                root: SymbolRoot::Root,
                path: "營收".into()
            }]
        );
        assert_eq!(
            tokenize(".données.résultat"),
            vec![Token::Symbol {
                root: SymbolRoot::Root,
                path: "données.résultat".into()
            }]
        );
    }

    #[test]
    fn tokenizes_element_symbol_with_at_sigil() {
        assert_eq!(
            tokenize("@"),
            vec![Token::Symbol {
                root: SymbolRoot::Element,
                path: "".into()
            }]
        );
        assert_eq!(
            tokenize("@.a"),
            vec![Token::Symbol {
                root: SymbolRoot::Element,
                path: "a".into()
            }]
        );
        assert_eq!(
            tokenize("@._0.name"),
            vec![Token::Symbol {
                root: SymbolRoot::Element,
                path: "_0.name".into()
            }]
        );
    }

    #[test]
    fn tokenizes_bool_and_null_literals() {
        assert_eq!(tokenize("True"), vec![Token::BoolLiteral(true)]);
        assert_eq!(tokenize("False"), vec![Token::BoolLiteral(false)]);
        assert_eq!(tokenize("Null"), vec![Token::NullLiteral]);
    }

    #[test]
    fn unknown_identifier_errors() {
        let err = Tokenizer::new("FooBar").tokenize().unwrap_err();
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn token_spans_are_byte_offsets() {
        let tokens = Tokenizer::new("(EQ 1 2)").tokenize().unwrap();
        // '(' at 0
        assert_eq!(tokens[0].span, Span::new(0, 1));
        // 'EQ' at 1..3
        assert_eq!(tokens[1].span, Span::new(1, 3));
        // '1' at 4..5
        assert_eq!(tokens[2].span, Span::new(4, 5));
        // '2' at 6..7
        assert_eq!(tokens[3].span, Span::new(6, 7));
        // ')' at 7..8
        assert_eq!(tokens[4].span, Span::new(7, 8));
    }

    // ── Parser tests ─────────────────────────────────────────

    fn must_parse(input: &str) -> Program {
        parse(input)
            .unwrap_or_else(|e| panic!("expected parse success for `{}`, got {:?}", input, e))
    }

    fn must_fail(input: &str) -> NightjarLanguageError {
        parse(input).expect_err(&format!("expected parse failure for `{}`", input))
    }

    #[test]
    fn parses_simple_verifier() {
        let p = must_parse("(GT 1 2)");
        match p.expr.node {
            BoolExpr::Verifier { op, .. } => assert_eq!(op, VerifierOp::GT),
            other => panic!("expected Verifier, got {:?}", other),
        }
    }

    #[test]
    fn verifier_arity_mismatch_produces_arity_error() {
        let err = must_fail("(GT 1 2 3)");
        assert!(
            matches!(err, NightjarLanguageError::ArgumentError { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn bare_gt_without_parens_fails() {
        let err = must_fail("GT 1 2");
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn parses_nested_connective_and_verifier() {
        let p = must_parse("(AND (GT 1 0) (LT 1 10))");
        match p.expr.node {
            BoolExpr::And(_, _) => {}
            other => panic!("expected And, got {:?}", other),
        }
    }

    #[test]
    fn parses_forall_with_partial_verifier() {
        let p = must_parse("(ForAll (GT 0) .ids)");
        match p.expr.node {
            BoolExpr::Quantifier {
                op,
                predicate,
                operand,
            } => {
                assert_eq!(op, QuantifierOp::ForAll);
                match predicate.node {
                    Predicate::PartialVerifier { op, .. } => assert_eq!(op, VerifierOp::GT),
                    other => panic!("expected PartialVerifier, got {:?}", other),
                }
                match operand.node {
                    ValueExpr::Symbol { root, path } => {
                        assert_eq!(root, SymbolRoot::Root);
                        assert_eq!(path, "ids");
                    }
                    other => panic!("expected Symbol, got {:?}", other),
                }
            }
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }

    #[test]
    fn parses_exists_with_nonempty_predicate() {
        let p = must_parse("(Exists NonEmpty .names)");
        match p.expr.node {
            BoolExpr::Quantifier { op, predicate, .. } => {
                assert_eq!(op, QuantifierOp::Exists);
                assert_eq!(
                    predicate.node,
                    Predicate::UnaryCheck(UnaryCheckOp::NonEmpty)
                );
            }
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }

    #[test]
    fn parses_not_of_verifier() {
        let p = must_parse("(NOT (EQ .status \"inactive\"))");
        match p.expr.node {
            BoolExpr::Not(inner) => match inner.node {
                BoolExpr::Verifier { op, .. } => assert_eq!(op, VerifierOp::EQ),
                other => panic!("expected Verifier, got {:?}", other),
            },
            other => panic!("expected Not, got {:?}", other),
        }
    }

    #[test]
    fn parses_top_level_bool_literal() {
        let p = must_parse("True");
        assert_eq!(p.expr.node, BoolExpr::Literal(true));
    }

    #[test]
    fn parses_negative_literals_in_verifier() {
        let p = must_parse("(GT -5 -10)");
        match p.expr.node {
            BoolExpr::Verifier { left, right, .. } => {
                assert_eq!(left.node, ValueExpr::Literal(Literal::Int(-5)));
                assert_eq!(right.node, ValueExpr::Literal(Literal::Int(-10)));
            }
            other => panic!("expected Verifier, got {:?}", other),
        }
    }

    #[test]
    fn parses_root_symbol_as_operand() {
        let p = must_parse("(NonEmpty .)");
        match p.expr.node {
            BoolExpr::UnaryCheck { op, operand } => {
                assert_eq!(op, UnaryCheckOp::NonEmpty);
                assert_eq!(
                    operand.node,
                    ValueExpr::Symbol {
                        root: SymbolRoot::Root,
                        path: "".into()
                    }
                );
            }
            other => panic!("expected UnaryCheck, got {:?}", other),
        }
    }

    #[test]
    fn func_call_arity_too_many_is_arity_error() {
        // Add has arity 2; the 3rd argument should be flagged as arity error.
        let err = must_fail("(EQ (Add 1 2 3) 6)");
        assert!(
            matches!(err, NightjarLanguageError::ArgumentError { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn func_call_arity_too_few_is_parse_error() {
        // Add has arity 2; only one operand given before `)` → parsing the 2nd
        // operand sees `)` which isn't a valid value expression.
        let err = must_fail("(EQ (Add 1) 1)");
        assert!(
            matches!(err, NightjarLanguageError::ParseError { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn missing_rparen_is_parse_error() {
        let err = must_fail("(GT 1 2");
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn trailing_tokens_is_parse_error() {
        let err = must_fail("(GT 1 2) extra");
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn empty_input_fails() {
        let err = must_fail("");
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn partial_verifier_outside_quantifier_is_rejected() {
        // (GT 2) in a full-verifier position is one operand short: the parser
        // sees `(GT <literal> )` with a second operand missing.
        let err = must_fail("(GT 2)");
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn depth_limit_is_enforced() {
        // Build ((((...True...))))-style expression wrapped in NOTs
        let mut s = String::new();
        let n = 10;
        for _ in 0..n {
            s.push_str("(NOT ");
        }
        s.push_str("True");
        for _ in 0..n {
            s.push(')');
        }
        // With very small max_depth it must fail.
        let cfg = ParserConfig { max_depth: 5 };
        let err = parse_with_config(&s, &cfg).unwrap_err();
        assert!(
            matches!(err, NightjarLanguageError::RecursionError { .. }),
            "got {:?}",
            err
        );
        // Default depth should comfortably parse it.
        parse(&s).expect("default depth should parse this");
    }

    #[test]
    fn parses_nested_arithmetic_inside_verifier() {
        let p = must_parse("(EQ (Add (Mul 2 3) (Sub 10 4)) 12)");
        match p.expr.node {
            BoolExpr::Verifier { left, .. } => match left.node {
                ValueExpr::FuncCall { op, args } => {
                    assert_eq!(op, FuncOp::Add);
                    assert_eq!(args.len(), 2);
                }
                other => panic!("expected FuncCall, got {:?}", other),
            },
            other => panic!("expected Verifier, got {:?}", other),
        }
    }

    #[test]
    fn parses_bool_literal_as_operand_to_eq() {
        let p = must_parse("(EQ True False)");
        match p.expr.node {
            BoolExpr::Verifier { left, right, .. } => {
                assert_eq!(left.node, ValueExpr::Literal(Literal::Bool(true)));
                assert_eq!(right.node, ValueExpr::Literal(Literal::Bool(false)));
            }
            other => panic!("expected Verifier, got {:?}", other),
        }
    }

    #[test]
    fn rejects_func_op_as_top_level_bool_expr() {
        // `(Add 1 2)` does not reduce to a boolean at top level.
        let err = must_fail("(Add 1 2)");
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn parses_unicode_symbol_in_verifier() {
        let p = must_parse("(EQ .數量 100)");
        match p.expr.node {
            BoolExpr::Verifier { left, .. } => {
                assert_eq!(
                    left.node,
                    ValueExpr::Symbol {
                        root: SymbolRoot::Root,
                        path: "數量".into()
                    }
                );
            }
            other => panic!("expected Verifier, got {:?}", other),
        }
    }

    #[test]
    fn top_level_span_covers_whole_expression() {
        let p = must_parse("(EQ 1 1)");
        assert_eq!(p.expr.span, Span::new(0, 8));
    }

    // ── Element-relative symbols (`@`) ───────────────────────

    #[test]
    fn rejects_at_followed_by_bare_identifier() {
        // `@a` has no `.` separator and is not legal.
        let err = Tokenizer::new("@a").tokenize().unwrap_err();
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn rejects_at_dot_with_no_segment() {
        let err = Tokenizer::new("@.").tokenize().unwrap_err();
        assert!(matches!(err, NightjarLanguageError::ParseError { .. }));
    }

    #[test]
    fn parses_forall_with_full_verifier_predicate() {
        let p = must_parse("(ForAll (EQ @.a @.b) .items)");
        match p.expr.node {
            BoolExpr::Quantifier { predicate, .. } => match predicate.node {
                Predicate::Full(body) => match body.node {
                    BoolExpr::Verifier { op, left, right } => {
                        assert_eq!(op, VerifierOp::EQ);
                        assert_eq!(
                            left.node,
                            ValueExpr::Symbol {
                                root: SymbolRoot::Element,
                                path: "a".into()
                            }
                        );
                        assert_eq!(
                            right.node,
                            ValueExpr::Symbol {
                                root: SymbolRoot::Element,
                                path: "b".into()
                            }
                        );
                    }
                    other => panic!("expected Verifier inside Full, got {:?}", other),
                },
                other => panic!("expected Predicate::Full, got {:?}", other),
            },
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }

    #[test]
    fn partial_verifier_still_parses_as_partial() {
        // Regression guard: `(GT 0)` in predicate position must still produce
        // Predicate::PartialVerifier, not Predicate::Full.
        let p = must_parse("(ForAll (GT 0) .items)");
        match p.expr.node {
            BoolExpr::Quantifier { predicate, .. } => {
                assert!(matches!(predicate.node, Predicate::PartialVerifier { .. }));
            }
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }

    #[test]
    fn parses_forall_with_nested_arithmetic_on_element() {
        let p = must_parse("(ForAll (EQ (Add @.a @.b) @.c) .items)");
        match p.expr.node {
            BoolExpr::Quantifier { predicate, .. } => {
                assert!(matches!(predicate.node, Predicate::Full(_)));
            }
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }

    #[test]
    fn rejects_at_outside_quantifier_with_scope_error() {
        let err = parse("(EQ @.a 1)").expect_err("`@` outside quantifier should fail");
        assert!(
            matches!(err, NightjarLanguageError::ScopeError { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn rejects_at_in_quantifier_operand_with_scope_error() {
        // `@` is legal only in the predicate — a quantifier's *operand* is
        // still resolved against the outer scope.
        let err = parse("(ForAll (GT 0) @.items)")
            .expect_err("`@` in quantifier operand at top level should fail");
        assert!(
            matches!(err, NightjarLanguageError::ScopeError { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn bare_at_symbol_parses_in_predicate() {
        let p = must_parse("(ForAll (GT @ 0) .scores)");
        match p.expr.node {
            BoolExpr::Quantifier { predicate, .. } => match predicate.node {
                Predicate::Full(body) => match body.node {
                    BoolExpr::Verifier { left, .. } => assert_eq!(
                        left.node,
                        ValueExpr::Symbol {
                            root: SymbolRoot::Element,
                            path: "".into()
                        }
                    ),
                    other => panic!("expected Verifier, got {:?}", other),
                },
                other => panic!("expected Full, got {:?}", other),
            },
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }

    #[test]
    fn nonempty_with_operand_in_predicate_parses_as_full() {
        // `(NonEmpty .x)` in predicate position is a full BoolExpr, not a
        // UnaryCheck predicate (which is the bare `NonEmpty` keyword form).
        let p = must_parse("(ForAll (NonEmpty .x) .items)");
        match p.expr.node {
            BoolExpr::Quantifier { predicate, .. } => match predicate.node {
                Predicate::Full(body) => assert!(matches!(
                    body.node,
                    BoolExpr::UnaryCheck {
                        op: UnaryCheckOp::NonEmpty,
                        ..
                    }
                )),
                other => panic!("expected Full, got {:?}", other),
            },
            other => panic!("expected Quantifier, got {:?}", other),
        }
    }
}
