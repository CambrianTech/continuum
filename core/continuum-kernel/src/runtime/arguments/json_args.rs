//! A `serde::Deserializer` over a borrowed [`serde_json::Value`] whose error type is OURS.
//!
//! serde_json's own `Value` deserializer reports every failure as `serde_json::Error`, a message
//! string: "missing field `x`" is text there, and reading the field back out of it is parsing
//! prose. serde's `de::Error` trait, though, hands the error type each failure as a typed call
//! (`missing_field(field)`, `invalid_type(unexpected, expected)`, `unknown_variant(..)`). This
//! deserializer is generic only so its error can be [`ArgError`], which keeps those calls as
//! variants. Paths come from `serde_path_to_error` wrapped around it.
//!
//! It borrows (no copy of the call's arguments to decode them), and the ROOT map can be
//! filtered, so the envelope's fields and a command's params decode from one object without
//! either seeing the other's keys.

use serde::de::value::{BorrowedStrDeserializer, MapDeserializer, SeqDeserializer};
use serde::de::{self, DeserializeSeed, EnumAccess, IntoDeserializer, VariantAccess, Visitor};
use serde::forward_to_deserialize_any;
use serde_json::Value;

/// One decode failure, as serde reported it to the error type: typed, never text, except
/// [`ArgError::Decoder`] (serde's `custom` channel, the only way a field type's own `Deserialize`
/// can speak; serde's API gives it nothing but a `Display`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArgError {
    Missing(&'static str),
    WrongType { expected: String },
    WrongValue { expected: String },
    WrongLength { len: usize, expected: String },
    UnknownVariant { variant: String, expected: &'static [&'static str] },
    UnknownField { field: String, expected: &'static [&'static str] },
    Duplicate(&'static str),
    Decoder(String),
}

impl std::fmt::Display for ArgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ArgError {}

impl de::Error for ArgError {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        ArgError::Decoder(msg.to_string())
    }
    fn invalid_type(_unexpected: de::Unexpected<'_>, expected: &dyn de::Expected) -> Self {
        ArgError::WrongType { expected: expected.to_string() }
    }
    fn invalid_value(_unexpected: de::Unexpected<'_>, expected: &dyn de::Expected) -> Self {
        ArgError::WrongValue { expected: expected.to_string() }
    }
    fn invalid_length(len: usize, expected: &dyn de::Expected) -> Self {
        ArgError::WrongLength { len, expected: expected.to_string() }
    }
    fn unknown_variant(variant: &str, expected: &'static [&'static str]) -> Self {
        ArgError::UnknownVariant { variant: variant.to_string(), expected }
    }
    fn unknown_field(field: &str, expected: &'static [&'static str]) -> Self {
        ArgError::UnknownField { field: field.to_string(), expected }
    }
    fn missing_field(field: &'static str) -> Self {
        ArgError::Missing(field)
    }
    fn duplicate_field(field: &'static str) -> Self {
        ArgError::Duplicate(field)
    }
}

/// A borrowed JSON value as a deserializer. `keep` filters the keys of THIS value's map (the
/// root's); nested values keep every key.
#[derive(Clone, Copy)]
pub(crate) struct JsonArgs<'de> {
    value: &'de Value,
    keep: fn(&str) -> bool,
}

fn every_key(_: &str) -> bool {
    true
}

impl<'de> JsonArgs<'de> {
    pub(crate) fn new(value: &'de Value) -> Self {
        Self { value, keep: every_key }
    }

    pub(crate) fn keeping(value: &'de Value, keep: fn(&str) -> bool) -> Self {
        Self { value, keep }
    }

    /// What she sent, for serde's `invalid_type` call. Its rendering is unused: the refusal
    /// reads the kind of the value at the failing path itself (`JsonKind::of`).
    fn unexpected(&self) -> de::Unexpected<'static> {
        de::Unexpected::Other("json value")
    }
}

impl<'de> IntoDeserializer<'de, ArgError> for JsonArgs<'de> {
    type Deserializer = Self;
    fn into_deserializer(self) -> Self {
        self
    }
}

impl<'de> de::Deserializer<'de> for JsonArgs<'de> {
    type Error = ArgError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ArgError> {
        match self.value {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::Number(n) => match (n.as_u64(), n.as_i64(), n.as_f64()) {
                (Some(u), _, _) => visitor.visit_u64(u),
                (None, Some(i), _) => visitor.visit_i64(i),
                (None, None, Some(f)) => visitor.visit_f64(f),
                (None, None, None) => Err(de::Error::invalid_type(self.unexpected(), &visitor)),
            },
            Value::String(s) => visitor.visit_borrowed_str(s),
            Value::Array(items) => {
                let mut seq = SeqDeserializer::new(items.iter().map(JsonArgs::new));
                let value = visitor.visit_seq(&mut seq)?;
                seq.end()?;
                Ok(value)
            }
            Value::Object(map) => {
                let keep = self.keep;
                let entries = map.iter().filter(move |(k, _)| keep(k)).map(|(k, v)| (BorrowedKey(k), JsonArgs::new(v)));
                let mut access = MapDeserializer::new(entries);
                let value = visitor.visit_map(&mut access)?;
                access.end()?;
                Ok(value)
            }
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ArgError> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, ArgError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, ArgError> {
        match self.value {
            Value::String(s) => visitor.visit_enum(BorrowedStrDeserializer::new(s)),
            Value::Object(map) => match (map.len(), map.iter().next()) {
                (1, Some((variant, value))) => visitor.visit_enum(VariantArgs { variant, value }),
                _ => Err(de::Error::invalid_type(self.unexpected(), &"a variant name or a one-key object")),
            },
            _ => Err(de::Error::invalid_type(self.unexpected(), &"a variant name or a one-key object")),
        }
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, ArgError> {
        visitor.visit_unit()
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf unit unit_struct seq tuple tuple_struct map struct identifier
    }
}

/// A map key, lent as a borrowed string (so a `&'de str` field and an identifier both read it).
struct BorrowedKey<'de>(&'de str);

impl<'de> IntoDeserializer<'de, ArgError> for BorrowedKey<'de> {
    type Deserializer = BorrowedStrDeserializer<'de, ArgError>;
    fn into_deserializer(self) -> Self::Deserializer {
        BorrowedStrDeserializer::new(self.0)
    }
}

/// An externally tagged enum written as `{ "variant": value }`.
struct VariantArgs<'de> {
    variant: &'de str,
    value: &'de Value,
}

impl<'de> EnumAccess<'de> for VariantArgs<'de> {
    type Error = ArgError;
    type Variant = Self;
    fn variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<(S::Value, Self), ArgError> {
        let variant = seed.deserialize(BorrowedStrDeserializer::<ArgError>::new(self.variant))?;
        Ok((variant, self))
    }
}

impl<'de> VariantAccess<'de> for VariantArgs<'de> {
    type Error = ArgError;
    fn unit_variant(self) -> Result<(), ArgError> {
        match self.value {
            Value::Null => Ok(()),
            _ => Err(de::Error::invalid_type(JsonArgs::new(self.value).unexpected(), &"a unit variant")),
        }
    }
    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value, ArgError> {
        seed.deserialize(JsonArgs::new(self.value))
    }
    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, ArgError> {
        de::Deserializer::deserialize_any(JsonArgs::new(self.value), visitor)
    }
    fn struct_variant<V: Visitor<'de>>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, ArgError> {
        de::Deserializer::deserialize_any(JsonArgs::new(self.value), visitor)
    }
}
