use async_graphql::{InputValueError, InputValueResult, Scalar, ScalarType, Value};
use borg_core::Uri;

/// Scalar wrapper around `borg_core::Uri`.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct UriScalar(pub Uri);

impl From<Uri> for UriScalar {
    fn from(value: Uri) -> Self {
        Self(value)
    }
}

impl From<UriScalar> for Uri {
    fn from(value: UriScalar) -> Self {
        value.0
    }
}

#[Scalar(name = "Uri")]
impl ScalarType for UriScalar {
    fn parse(value: Value) -> InputValueResult<Self> {
        match value {
            Value::String(raw) => Uri::parse(&raw)
                .map(Self)
                .map_err(|err| InputValueError::custom(err.to_string())),
            other => Err(InputValueError::expected_type(other)),
        }
    }

    fn to_value(&self) -> Value {
        Value::String(self.0.to_string())
    }
}

/// Transitional scalar for fields that still map to legacy JSON columns.
#[derive(Clone, Debug, PartialEq)]
pub struct JsonValue(pub serde_json::Value);

#[Scalar(name = "JsonValue")]
impl ScalarType for JsonValue {
    fn parse(value: Value) -> InputValueResult<Self> {
        let as_json = value
            .into_json()
            .map_err(|err| InputValueError::custom(err.to_string()))?;
        Ok(Self(as_json))
    }

    fn to_value(&self) -> Value {
        Value::from_json(self.0.clone()).unwrap_or(Value::Null)
    }
}
