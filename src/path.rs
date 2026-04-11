use std::fmt;

use maxminddb::PathElement;
use serde::de::{Deserialize, Deserializer, Visitor};

/// A leaf value extracted from an MMDB record via [`maxminddb::LookupResult::decode_path`].
///
/// Operators address arbitrary fields in MMDB records by configuring a path; the
/// underlying value can be any of the primitive MMDB types. `AnyLeaf` is the
/// `T` argument to `decode_path::<AnyLeaf>` so a single call covers all of them.
///
/// Non-primitive landings (map, array, bytes) cause `decode_path` to return
/// `Err(MaxMindDbError::Decoding{..})`, which the caller maps to a configuration
/// error rather than a per-request data anomaly.
#[derive(Debug, Clone, PartialEq)]
pub enum AnyLeaf {
    Str(String),
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
}

impl AnyLeaf {
    /// Render the leaf as a string for emission as an SPOE transaction variable.
    ///
    /// Strings pass through verbatim; integers and floats use Rust's `Display`
    /// (canonical decimal); booleans become the literal `"true"` / `"false"`.
    pub fn into_string(self) -> String {
        match self {
            Self::Str(s) => s,
            Self::Int(i) => i.to_string(),
            Self::UInt(u) => u.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => if b { "true" } else { "false" }.to_string(),
        }
    }
}

impl<'de> Deserialize<'de> for AnyLeaf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(AnyLeafVisitor)
    }
}

struct AnyLeafVisitor;

impl<'de> Visitor<'de> for AnyLeafVisitor {
    type Value = AnyLeaf;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a primitive MMDB leaf value (string, integer, float, or boolean)")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Int(v))
    }

    fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Int(v as i64))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
        Ok(AnyLeaf::UInt(v))
    }

    fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E> {
        Ok(AnyLeaf::UInt(v as u64))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Float(v))
    }

    fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Float(v as f64))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Str(v.to_owned()))
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Str(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E> {
        Ok(AnyLeaf::Str(v))
    }
}

/// Build the `PathElement` slice required by [`maxminddb::LookupResult::decode_path`]
/// from an operator-supplied list of map keys. The returned vector borrows from
/// `keys`, so it must not outlive the input.
pub fn compile_path(keys: &[String]) -> Vec<PathElement<'_>> {
    keys.iter().map(|k| PathElement::Key(k.as_str())).collect()
}

/// Render a field path as a slash-separated display string for use in log
/// messages and as a stable cache key for warn-once deduplication.
pub fn path_display(keys: &[String]) -> String {
    keys.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use maxminddb::PathElement;
    use serde::de::IntoDeserializer;
    use serde::de::value::{
        BoolDeserializer, Error, F64Deserializer, I64Deserializer, StrDeserializer, U64Deserializer,
    };

    #[test]
    fn any_leaf_deserializes_string_value() {
        let de: StrDeserializer<'_, Error> = "hello".into_deserializer();
        let leaf = AnyLeaf::deserialize(de).unwrap();
        assert_eq!(leaf, AnyLeaf::Str("hello".into()));
        assert_eq!(leaf.into_string(), "hello");
    }

    #[test]
    fn any_leaf_deserializes_signed_int_value() {
        let de: I64Deserializer<Error> = (-42i64).into_deserializer();
        let leaf = AnyLeaf::deserialize(de).unwrap();
        assert_eq!(leaf, AnyLeaf::Int(-42));
        assert_eq!(leaf.into_string(), "-42");
    }

    #[test]
    fn any_leaf_deserializes_unsigned_int_value() {
        let de: U64Deserializer<Error> = 29518u64.into_deserializer();
        let leaf = AnyLeaf::deserialize(de).unwrap();
        assert_eq!(leaf, AnyLeaf::UInt(29518));
        assert_eq!(leaf.into_string(), "29518");
    }

    #[test]
    fn any_leaf_deserializes_float_value() {
        let de: F64Deserializer<Error> = 51.5074f64.into_deserializer();
        let leaf = AnyLeaf::deserialize(de).unwrap();
        assert_eq!(leaf, AnyLeaf::Float(51.5074));
        assert_eq!(leaf.into_string(), "51.5074");
    }

    #[test]
    fn any_leaf_deserializes_true_bool_value() {
        let de: BoolDeserializer<Error> = true.into_deserializer();
        let leaf = AnyLeaf::deserialize(de).unwrap();
        assert_eq!(leaf, AnyLeaf::Bool(true));
        assert_eq!(leaf.into_string(), "true");
    }

    #[test]
    fn any_leaf_deserializes_false_bool_value() {
        let de: BoolDeserializer<Error> = false.into_deserializer();
        let leaf = AnyLeaf::deserialize(de).unwrap();
        assert_eq!(leaf.into_string(), "false");
    }

    #[test]
    fn compile_path_round_trips_three_key_path() {
        let keys = vec!["country".to_string(), "names".to_string(), "de".to_string()];
        let elements = compile_path(&keys);
        assert_eq!(elements.len(), 3);
        assert!(matches!(elements[0], PathElement::Key("country")));
        assert!(matches!(elements[1], PathElement::Key("names")));
        assert!(matches!(elements[2], PathElement::Key("de")));
    }

    #[test]
    fn path_display_joins_with_slash() {
        let keys = vec!["country".to_string(), "iso_code".to_string()];
        assert_eq!(path_display(&keys), "country/iso_code");
    }
}
