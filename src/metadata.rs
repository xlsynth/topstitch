// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fmt;

pub type MetadataKey = String;

#[derive(Debug, Clone, PartialEq)]
pub enum MetadataValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

pub type Metadata = HashMap<MetadataKey, MetadataValue>;

// type_name

impl MetadataValue {
    pub fn type_name(&self) -> &'static str {
        match self {
            MetadataValue::String(_) => "String",
            MetadataValue::Int(_) => "Int",
            MetadataValue::Float(_) => "Float",
            MetadataValue::Bool(_) => "Bool",
        }
    }
}

// Display

impl fmt::Display for MetadataValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetadataValue::String(s) => write!(f, "{s}"),
            MetadataValue::Int(v) => write!(f, "{v}"),
            MetadataValue::Float(v) => write!(f, "{v}"),
            MetadataValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

// T -> MetadataValue

impl From<String> for MetadataValue {
    fn from(v: String) -> Self {
        MetadataValue::String(v)
    }
}

impl From<&str> for MetadataValue {
    fn from(v: &str) -> Self {
        MetadataValue::String(v.to_owned())
    }
}

impl From<i64> for MetadataValue {
    fn from(v: i64) -> Self {
        MetadataValue::Int(v)
    }
}

impl From<f64> for MetadataValue {
    fn from(v: f64) -> Self {
        MetadataValue::Float(v)
    }
}

impl From<bool> for MetadataValue {
    fn from(v: bool) -> Self {
        MetadataValue::Bool(v)
    }
}

// MetadataValue -> T

impl From<MetadataValue> for String {
    fn from(v: MetadataValue) -> Self {
        match v {
            MetadataValue::String(s) => s,
            other => panic!(
                "Expected String metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

impl From<&MetadataValue> for String {
    fn from(v: &MetadataValue) -> Self {
        match v {
            MetadataValue::String(s) => s.clone(),
            other => panic!(
                "Expected String metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

impl From<MetadataValue> for i64 {
    fn from(v: MetadataValue) -> Self {
        match v {
            MetadataValue::Int(x) => x,
            other => panic!(
                "Expected Int metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}
impl From<&MetadataValue> for i64 {
    fn from(v: &MetadataValue) -> Self {
        match v {
            MetadataValue::Int(x) => *x,
            other => panic!(
                "Expected Int metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

impl From<MetadataValue> for f64 {
    fn from(v: MetadataValue) -> Self {
        match v {
            MetadataValue::Float(x) => x,
            other => panic!(
                "Expected Float metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

impl From<&MetadataValue> for f64 {
    fn from(v: &MetadataValue) -> Self {
        match v {
            MetadataValue::Float(x) => *x,
            other => panic!(
                "Expected Float metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

impl From<MetadataValue> for bool {
    fn from(v: MetadataValue) -> Self {
        match v {
            MetadataValue::Bool(x) => x,
            other => panic!(
                "Expected Bool metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

impl From<&MetadataValue> for bool {
    fn from(v: &MetadataValue) -> Self {
        match v {
            MetadataValue::Bool(x) => *x,
            other => panic!(
                "Expected Bool metadata, got {} ({})",
                other.type_name(),
                other
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_primitives_into_metadata_value() {
        let v: MetadataValue = "hello".into();
        assert_eq!(v, MetadataValue::String("hello".to_string()));

        let v: MetadataValue = String::from("world").into();
        assert_eq!(v, MetadataValue::String("world".to_string()));

        let v: MetadataValue = 42_i64.into();
        assert_eq!(v, MetadataValue::Int(42_i64));

        let v: MetadataValue = 3.25_f64.into();
        assert_eq!(v, MetadataValue::Float(3.25_f64));

        let v: MetadataValue = true.into();
        assert_eq!(v, MetadataValue::Bool(true));
    }

    #[test]
    fn into_primitives_from_metadata_value_by_ref() {
        let s: String = (&MetadataValue::String("abc".to_string())).into();
        assert_eq!(s, "abc");

        let i: i64 = (&MetadataValue::Int(-7)).into();
        assert_eq!(i, -7);

        let f: f64 = (&MetadataValue::Float(1.5_f64)).into();
        assert_eq!(f, 1.5_f64);

        let b: bool = (&MetadataValue::Bool(true)).into();
        assert!(b);
    }

    #[test]
    fn into_primitives_from_metadata_value_by_value() {
        let s: String = MetadataValue::String("abc".into()).into();
        assert_eq!(s, "abc");

        let i: i64 = MetadataValue::Int(99).into();
        assert_eq!(i, 99);

        let f: f64 = MetadataValue::Float(2.0).into();
        assert_eq!(f, 2.0);

        let b: bool = MetadataValue::Bool(false).into();
        assert!(!b);
    }

    #[test]
    #[should_panic]
    fn panics_on_wrong_type_int_from_string_ref() {
        let _x: i64 = (&MetadataValue::String("nope".into())).into();
    }

    #[test]
    #[should_panic]
    fn panics_on_wrong_type_string_from_int_value() {
        let _s: String = MetadataValue::Int(123).into();
    }
}
