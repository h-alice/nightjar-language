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
// Crate root. Re-exports the Phase 1 (parser) and Phase 2 (executor/runtime)
// public surfaces of the Nightjar verification DSL.

//! Nightjar Language — a declarative, prefix-notation DSL for formal
//! verification of structured data.

pub mod context;
pub mod error;
pub mod executor;
pub mod language;
pub mod symbol_table;

// ── Phase 1 (parser) public surface ──────────────────────────────
pub use error::{ErrorCode, NightjarLanguageError, Span};
pub use language::grammar::{
    BoolExpr, FuncOp, Keyword, Literal, Predicate, Program, QuantifierOp, Spanned, SpannedBoolExpr,
    SpannedValueExpr, UnaryCheckOp, ValueExpr, VerifierOp,
};
pub use language::parser::{parse, parse_with_config, ParserConfig};

// ── Phase 2 (runtime + executor) public surface ──────────────────
pub use context::entity::{Entity, TypeTag};
pub use executor::{exec_entity, ExecOptions, ExecResult};
pub use symbol_table::SymbolTable;

#[cfg(feature = "json")]
pub use executor::exec;
