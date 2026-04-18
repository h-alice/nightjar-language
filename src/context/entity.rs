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

//! Entity module.
//!
//! Entity is the core tagged data value (Int, Float, String, Bool, List, Map, Null)
//! with its TypeTag enum and conversions used throughout the runtime.

use std::collections::HashMap;
use std::fmt;

/// Type tags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeTag {
    Int,
    Float,
    String,
    Bool,
    List,
    Map,
    Null,
}

impl fmt::Display for TypeTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TypeTag::Int => "Int",
            TypeTag::Float => "Float",
            TypeTag::String => "String",
            TypeTag::Bool => "Bool",
            TypeTag::List => "List",
            TypeTag::Map => "Map",
            TypeTag::Null => "Null",
        };
        f.write_str(s)
    }
}

/// The fundamental data-holding unit.
#[derive(Debug, Clone, PartialEq)]
pub enum Entity {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<Entity>),
    Map(HashMap<String, Entity>),
    Null,
}

impl Entity {
    pub fn type_tag(&self) -> TypeTag {
        match self {
            Entity::Int(_) => TypeTag::Int,
            Entity::Float(_) => TypeTag::Float,
            Entity::String(_) => TypeTag::String,
            Entity::Bool(_) => TypeTag::Bool,
            Entity::List(_) => TypeTag::List,
            Entity::Map(_) => TypeTag::Map,
            Entity::Null => TypeTag::Null,
        }
    }

    /// NonEmpty check
    ///
    /// - `String`/`List`/`Map` are empty when their container is empty.
    /// - `Null` is always empty.
    /// - Scalars (`Int`, `Float`, `Bool`) are always non-empty.
    pub fn is_non_empty(&self) -> bool {
        match self {
            Entity::String(s) => !s.is_empty(),
            Entity::List(v) => !v.is_empty(),
            Entity::Map(m) => !m.is_empty(),
            Entity::Null => false,
            Entity::Int(_) | Entity::Float(_) | Entity::Bool(_) => true,
        }
    }
}

// ─────────────────── promote to entity type ───────────────────

impl From<i64> for Entity {
    fn from(v: i64) -> Self {
        Entity::Int(v)
    }
}

impl From<f64> for Entity {
    fn from(v: f64) -> Self {
        Entity::Float(v)
    }
}

impl From<String> for Entity {
    fn from(v: String) -> Self {
        Entity::String(v)
    }
}

impl From<&str> for Entity {
    fn from(v: &str) -> Self {
        Entity::String(v.to_string())
    }
}

impl From<bool> for Entity {
    fn from(v: bool) -> Self {
        Entity::Bool(v)
    }
}

#[cfg(feature = "json")]
impl From<serde_json::Value> for Entity {
    fn from(val: serde_json::Value) -> Self {
        match val {
            serde_json::Value::Null => Entity::Null,
            serde_json::Value::Bool(b) => Entity::Bool(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Entity::Int(i)
                } else if let Some(f) = n.as_f64() {
                    Entity::Float(f)
                } else {
                    // Extremely large u64 values that don't fit in i64 or f64:
                    // fall back to 0 via f64
                    //
                    //practically unreachable, who would use such large values?
                    Entity::Float(0.0)
                }
            }
            serde_json::Value::String(s) => Entity::String(s),
            serde_json::Value::Array(arr) => {
                Entity::List(arr.into_iter().map(Entity::from).collect())
            }
            serde_json::Value::Object(map) => {
                Entity::Map(map.into_iter().map(|(k, v)| (k, Entity::from(v))).collect())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_tag_for_every_variant() {
        assert_eq!(Entity::Int(0).type_tag(), TypeTag::Int);
        assert_eq!(Entity::Float(0.0).type_tag(), TypeTag::Float);
        assert_eq!(Entity::String("".into()).type_tag(), TypeTag::String);
        assert_eq!(Entity::Bool(false).type_tag(), TypeTag::Bool);
        assert_eq!(Entity::List(vec![]).type_tag(), TypeTag::List);
        assert_eq!(Entity::Map(HashMap::new()).type_tag(), TypeTag::Map);
        assert_eq!(Entity::Null.type_tag(), TypeTag::Null);
    }

    #[test]
    fn display_impl_for_typetag() {
        assert_eq!(format!("{}", TypeTag::Int), "Int");
        assert_eq!(format!("{}", TypeTag::Null), "Null");
        assert_eq!(format!("{}", TypeTag::Map), "Map");
    }

    #[test]
    fn is_non_empty_for_scalars_always_true() {
        assert!(Entity::Int(0).is_non_empty());
        assert!(Entity::Float(0.0).is_non_empty());
        assert!(Entity::Bool(false).is_non_empty());
    }

    #[test]
    fn is_non_empty_for_null_false() {
        assert!(!Entity::Null.is_non_empty());
    }

    #[test]
    fn is_non_empty_for_containers() {
        assert!(!Entity::String("".into()).is_non_empty());
        assert!(Entity::String("a".into()).is_non_empty());
        assert!(!Entity::List(vec![]).is_non_empty());
        assert!(Entity::List(vec![Entity::Int(1)]).is_non_empty());
        assert!(!Entity::Map(HashMap::new()).is_non_empty());
        let mut m = HashMap::new();
        m.insert("k".to_string(), Entity::Int(1));
        assert!(Entity::Map(m).is_non_empty());
    }

    #[test]
    fn from_primitives() {
        assert_eq!(Entity::from(5_i64), Entity::Int(5));
        assert_eq!(Entity::from(2.5_f64), Entity::Float(2.5));
        assert_eq!(Entity::from(true), Entity::Bool(true));
        assert_eq!(Entity::from("hello"), Entity::String("hello".to_string()));
        assert_eq!(
            Entity::from(String::from("world")),
            Entity::String("world".to_string())
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn from_json_scalars() {
        use serde_json::json;
        assert_eq!(Entity::from(json!(null)), Entity::Null);
        assert_eq!(Entity::from(json!(true)), Entity::Bool(true));
        assert_eq!(Entity::from(json!(42)), Entity::Int(42));
        assert_eq!(Entity::from(json!(-7)), Entity::Int(-7));
        assert_eq!(Entity::from(json!(3.14)), Entity::Float(3.14));
        assert_eq!(
            Entity::from(json!("abc")),
            Entity::String("abc".to_string())
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn from_json_nested_object_and_array() {
        use serde_json::json;
        let j = json!({
            "data": {
                "department_1": { "revenue": 100 },
                "department_2": { "revenue": 200 }
            },
            "id_list": [10, 20, 30]
        });
        let ent = Entity::from(j);
        match ent {
            Entity::Map(map) => {
                assert_eq!(map.len(), 2);
                assert!(matches!(map.get("data"), Some(Entity::Map(_))));
                if let Some(Entity::List(ids)) = map.get("id_list") {
                    assert_eq!(ids.len(), 3);
                    assert_eq!(ids[0], Entity::Int(10));
                } else {
                    panic!("id_list should be a list");
                }
            }
            other => panic!("expected Map, got {:?}", other),
        }
    }

    #[cfg(feature = "json")]
    #[test]
    fn from_json_preserves_unicode_keys() {
        use serde_json::json;
        let j = json!({ "營收": 500 });
        let ent = Entity::from(j);
        if let Entity::Map(map) = ent {
            assert_eq!(map.get("營收"), Some(&Entity::Int(500)));
        } else {
            panic!("expected Map");
        }
    }
}
