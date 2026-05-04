# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-04

### Added

- Initial release of Nightjar Language, a declarative, prefix-notation DSL
  for formal verification of structured data, delivered as an embeddable Rust
  library.
- Three-valued execution result (`True` / `False` / `Error`) where every
  error carries a stable code, a source span, and a human-readable message.
- Verifier (`EQ`, `NE`, `LT`, `LE`, `GT`, `GE`), unary check (`NonEmpty`),
  connective (`AND`, `OR`, `NOT`), and quantifier (`ForAll`, `Exists`)
  operators.
- Arithmetic (`Add`, `Sub`, `Mul`, `Div`, `Mod`, `Neg`, `Abs`), string
  (`Concat`, `Length`, `Substring`, `Upper`, `Lower`), and collection
  (`Head`, `Tail`, `Get`, `Count`, `GetKeys`, `GetValues`) functions.
- Root-rooted (`.`) and element-rooted (`@`) symbol namespaces with
  Unicode-aware path segments.
- Configurable `ExecOptions` with a float epsilon for `EQ`/`NE` and a
  parser nesting-depth guard.
- `json` (default) and `yaml` cargo features. With `json`, the convenience
  `exec(expr, serde_json::Value, opts)` entry point and a
  `From<serde_json::Value> for Entity` impl are available.
- Ten stable error codes (`E001`..`E010`).

[Unreleased]: https://github.com/h-alice/nightjar-language/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/h-alice/nightjar-language/releases/tag/v0.1.0
