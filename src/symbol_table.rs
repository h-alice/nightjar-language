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

//! Symbol table.
//!
//! The symbol table uses flattened design, which provides O(1) access to
//! any symbol in the input data.
//!
//! It is built by recursively traversing the input data and storing every node
//! in a hash map. The keys of the hash map are the dot-separated paths to the
//! nodes, and the values are the nodes themselves.

use std::collections::HashMap;

use crate::context::entity::Entity;
use crate::error::{symbol_not_found, type_error, NightjarLanguageError, Span};

/// A flattened mapping from dot-separated symbol paths to their Entity values.
///
/// Construction is recursive, every node in the input data (including
/// intermediate maps/lists) is registered under its fully qualified path.
///
/// Lists are unrolled with 0-based `_N` segments.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    entries: HashMap<String, Entity>,
}

impl SymbolTable {
    /// Build a symbol table from a root Entity.
    pub fn from_entity(root: Entity) -> Self {
        let mut entries = HashMap::new();
        entries.insert(".".to_string(), root.clone());
        flatten(&mut entries, ".", &root);
        Self { entries }
    }

    /// Strict resolution.
    ///
    /// The symbol path must match an exact key.
    pub fn resolve(&self, symbol: &str, span: Span) -> Result<Entity, NightjarLanguageError> {
        self.entries
            .get(symbol)
            .cloned()
            .ok_or_else(|| symbol_not_found(span, symbol))
    }

    /// Root symbol resolution.
    ///
    /// Resolve a root-rooted path given *without* the leading `.` sigil.
    ///
    /// An empty path returns the root entity itself.
    ///
    /// Primarily used by the executor when materializing `ValueExpr::Symbol { root: Root, path }`.
    pub fn resolve_root_path(
        &self,
        path: &str,
        span: Span,
    ) -> Result<Entity, NightjarLanguageError> {
        let key = if path.is_empty() {
            ".".to_string()
        } else {
            format!(".{}", path)
        };
        self.resolve(&key, span)
    }

    /// Size of the flattened table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the flattened table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether a specific path was registered.
    pub fn contains(&self, symbol: &str) -> bool {
        self.entries.contains_key(symbol)
    }
}

#[cfg(feature = "json")]
impl SymbolTable {
    /// Construct Entity from JSON, then flatten.
    pub fn from_json(value: serde_json::Value) -> Self {
        Self::from_entity(Entity::from(value))
    }
}

/// Resolve a path against **an entity** (not globally).
///
/// Walk a dot-separated path against a given `Entity` root, returning the
/// addressed value. `path` must *not* carry a leading sigil — an empty path
/// returns the root itself.
///
/// Segments beginning with `_` are treated as list indices (e.g. `_0`, `_12`),
/// matching the flattening convention used by [`SymbolTable::from_entity`].
///
/// Any other traversal shape (indexing into a scalar, missing key, missing index)
/// results in an appropriate [`NightjarLanguageError`] carrying the supplied `span`.
///
/// Used by the executor to resolve element-relative symbols (`@.a.b`) against
/// the current iteration element without rebuilding a full [`SymbolTable`].
pub fn resolve_in_entity(
    path: &str,
    entity: &Entity,
    span: Span,
) -> Result<Entity, NightjarLanguageError> {
    if path.is_empty() {
        return Ok(entity.clone()); // Empty path returns the root entity itself.
    }
    let mut cur: Entity = entity.clone(); // Indicates the current entity being resolved.
    for seg in path.split('.') {
        // Walk until the end of the path, or error.
        cur = match cur {
            Entity::Map(ref m) => m.get(seg).cloned().ok_or_else(|| {
                symbol_not_found(span, &format!("@ element missing field `{}`", seg))
            })?,
            Entity::List(ref l) if seg.starts_with('_') => {
                let idx: usize = seg[1..].parse().map_err(|_| {
                    symbol_not_found(span, &format!("@ element index `{}` invalid", seg))
                })?;
                l.get(idx).cloned().ok_or_else(|| {
                    symbol_not_found(span, &format!("@ element index `_{}` out of bounds", idx))
                })?
            }
            _ => {
                return Err(type_error(
                    span,
                    format!("cannot index into non-container at segment `{}`", seg),
                ));
            }
        };
    }
    Ok(cur)
}

/// Flatten an entity into a symbol table.
///
/// Recursively register every path in `entity` under `prefix` into `table`.
///
/// For `Map` children the key is appended after a `.`.
///
/// For `List` children the 0-based index `i` is appended as `._i`.
///
/// Scalars and `Null` are not descended into, the caller is expected to have
/// already registered them at `prefix`.
///
/// This is the workhorse behind [`SymbolTable::from_entity`]: it produces
/// the O(1) flat lookup table that the executor uses for root-rooted
/// symbol resolution.
///
/// Example (internal; private helper, run via the public `SymbolTable`):
///
/// ```ignore
/// // Conceptually: flattening `{data: {rev: 100}}` registers
/// //   "."             → Map{..}
/// //   ".data"         → Map{rev:100}
/// //   ".data.rev"     → Int(100)
/// let mut table = std::collections::HashMap::new();
/// table.insert(".".into(), root.clone());
/// flatten(&mut table, ".", &root);
/// ```
fn flatten(table: &mut HashMap<String, Entity>, prefix: &str, entity: &Entity) {
    match entity {
        Entity::Map(map) => {
            for (key, value) in map {
                let path = if prefix == "." {
                    format!(".{}", key)
                } else {
                    format!("{}.{}", prefix, key)
                };
                table.insert(path.clone(), value.clone());
                flatten(table, &path, value); // Recursive call
            }
        }
        Entity::List(list) => {
            for (idx, value) in list.iter().enumerate() {
                let path = if prefix == "." {
                    format!("._{}", idx)
                } else {
                    format!("{}._{}", prefix, idx)
                };
                table.insert(path.clone(), value.clone());
                flatten(table, &path, value); // Recursive call
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map_of(pairs: &[(&str, Entity)]) -> Entity {
        let mut m = HashMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        Entity::Map(m)
    }

    #[test]
    fn empty_map_has_only_root() {
        let st = SymbolTable::from_entity(Entity::Map(HashMap::new()));
        assert!(st.contains("."));
        assert_eq!(st.len(), 1);
    }

    #[test]
    fn root_symbol_returns_root_entity() {
        let root = map_of(&[("revenue", Entity::Int(100))]);
        let st = SymbolTable::from_entity(root.clone());
        assert_eq!(st.resolve(".", Span::new(0, 0)).unwrap(), root);
    }

    #[test]
    fn flat_map_registers_leaf() {
        let root = map_of(&[("revenue", Entity::Int(100))]);
        let st = SymbolTable::from_entity(root);
        assert_eq!(
            st.resolve(".revenue", Span::new(0, 0)).unwrap(),
            Entity::Int(100)
        );
    }

    #[test]
    fn nested_map_registers_intermediate_and_leaf() {
        let inner = map_of(&[("revenue", Entity::Int(100))]);
        let root = map_of(&[("data", map_of(&[("dept", inner.clone())]))]);
        let st = SymbolTable::from_entity(root);
        // Every level should be addressable.
        assert!(st.contains(".data"));
        assert!(st.contains(".data.dept"));
        assert!(st.contains(".data.dept.revenue"));
        assert_eq!(
            st.resolve(".data.dept.revenue", Span::new(0, 0)).unwrap(),
            Entity::Int(100)
        );
    }

    #[test]
    fn list_is_unrolled_with_zero_based_underscore_prefix() {
        let list = Entity::List(vec![Entity::Int(10), Entity::Int(20), Entity::Int(30)]);
        let root = map_of(&[("ids", list.clone())]);
        let st = SymbolTable::from_entity(root);
        assert_eq!(st.resolve(".ids", Span::new(0, 0)).unwrap(), list);
        assert_eq!(
            st.resolve(".ids._0", Span::new(0, 0)).unwrap(),
            Entity::Int(10)
        );
        assert_eq!(
            st.resolve(".ids._1", Span::new(0, 0)).unwrap(),
            Entity::Int(20)
        );
        assert_eq!(
            st.resolve(".ids._2", Span::new(0, 0)).unwrap(),
            Entity::Int(30)
        );
    }

    #[test]
    fn nested_list_flattens_recursively() {
        let matrix = Entity::List(vec![
            Entity::List(vec![Entity::Int(1), Entity::Int(2)]),
            Entity::List(vec![Entity::Int(3), Entity::Int(4)]),
        ]);
        let root = map_of(&[("m", matrix)]);
        let st = SymbolTable::from_entity(root);
        assert_eq!(
            st.resolve(".m._0._0", Span::new(0, 0)).unwrap(),
            Entity::Int(1)
        );
        assert_eq!(
            st.resolve(".m._0._1", Span::new(0, 0)).unwrap(),
            Entity::Int(2)
        );
        assert_eq!(
            st.resolve(".m._1._0", Span::new(0, 0)).unwrap(),
            Entity::Int(3)
        );
        assert_eq!(
            st.resolve(".m._1._1", Span::new(0, 0)).unwrap(),
            Entity::Int(4)
        );
    }

    #[test]
    fn missing_symbol_errors() {
        let st = SymbolTable::from_entity(map_of(&[("x", Entity::Int(1))]));
        let err = st.resolve(".missing", Span::new(0, 0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::SymbolNotFound { .. }));
    }

    #[test]
    fn list_of_maps_flattens() {
        let root = map_of(&[(
            "users",
            Entity::List(vec![
                map_of(&[("name", Entity::String("a".into()))]),
                map_of(&[("name", Entity::String("b".into()))]),
            ]),
        )]);
        let st = SymbolTable::from_entity(root);
        assert_eq!(
            st.resolve(".users._0.name", Span::new(0, 0)).unwrap(),
            Entity::String("a".into())
        );
        assert_eq!(
            st.resolve(".users._1.name", Span::new(0, 0)).unwrap(),
            Entity::String("b".into())
        );
    }

    #[test]
    fn unicode_key_is_addressable() {
        let root = map_of(&[("營收", Entity::Int(500))]);
        let st = SymbolTable::from_entity(root);
        assert!(st.contains(".營收"));
        assert_eq!(
            st.resolve(".營收", Span::new(0, 0)).unwrap(),
            Entity::Int(500)
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn from_json_end_to_end() {
        let j = serde_json::json!({
            "data": {
                "department_1": { "revenue": 100 },
                "department_2": { "revenue": 200 }
            }
        });
        let st = SymbolTable::from_json(j);
        assert_eq!(
            st.resolve(".data.department_1.revenue", Span::new(0, 0))
                .unwrap(),
            Entity::Int(100)
        );
        assert_eq!(
            st.resolve(".data.department_2.revenue", Span::new(0, 0))
                .unwrap(),
            Entity::Int(200)
        );
    }

    // ── resolve_in_entity ────────────────────────────────────

    #[test]
    fn resolve_in_entity_empty_path_returns_root() {
        let elem = Entity::Int(42);
        assert_eq!(
            resolve_in_entity("", &elem, Span::new(0, 0)).unwrap(),
            Entity::Int(42)
        );
    }

    #[test]
    fn resolve_in_entity_walks_map_fields() {
        let elem = map_of(&[("a", Entity::Int(1)), ("b", Entity::Int(2))]);
        assert_eq!(
            resolve_in_entity("a", &elem, Span::new(0, 0)).unwrap(),
            Entity::Int(1)
        );
        assert_eq!(
            resolve_in_entity("b", &elem, Span::new(0, 0)).unwrap(),
            Entity::Int(2)
        );
    }

    #[test]
    fn resolve_in_entity_nested_map() {
        let inner = map_of(&[("x", Entity::Int(9))]);
        let elem = map_of(&[("nested", inner)]);
        assert_eq!(
            resolve_in_entity("nested.x", &elem, Span::new(0, 0)).unwrap(),
            Entity::Int(9)
        );
    }

    #[test]
    fn resolve_in_entity_list_index_segment() {
        let elem = Entity::List(vec![Entity::Int(10), Entity::Int(20)]);
        assert_eq!(
            resolve_in_entity("_1", &elem, Span::new(0, 0)).unwrap(),
            Entity::Int(20)
        );
    }

    #[test]
    fn resolve_in_entity_missing_field_errors() {
        let elem = map_of(&[("a", Entity::Int(1))]);
        let err = resolve_in_entity("b", &elem, Span::new(0, 0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::SymbolNotFound { .. }));
    }

    #[test]
    fn resolve_in_entity_scalar_traversal_errors() {
        let err = resolve_in_entity("a", &Entity::Int(5), Span::new(0, 0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::TypeError { .. }));
    }

    #[test]
    fn resolve_in_entity_index_out_of_bounds() {
        let elem = Entity::List(vec![Entity::Int(10)]);
        let err = resolve_in_entity("_5", &elem, Span::new(0, 0)).unwrap_err();
        assert!(matches!(err, NightjarLanguageError::SymbolNotFound { .. }));
    }

    #[cfg(feature = "json")]
    #[test]
    fn example_matches_symbol_table_layout() {
        // The flattened table must include both intermediate
        // maps and leaf scalars under their fully qualified paths.
        let j = serde_json::json!({
            "data": {
                "department_1": { "revenue": 100 },
                "department_2": { "revenue": 200 }
            }
        });
        let st = SymbolTable::from_json(j);
        for key in [
            ".",
            ".data",
            ".data.department_1",
            ".data.department_1.revenue",
            ".data.department_2",
            ".data.department_2.revenue",
        ] {
            assert!(st.contains(key), "missing key `{}`", key);
        }
    }
}
