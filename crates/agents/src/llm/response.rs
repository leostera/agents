use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::marker::PhantomData;

/// Provider-facing response format payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawResponseFormat {
    pub r#type: String,
    pub json_schema: Option<RawResponseJsonSchema>,
}

/// JSON Schema payload embedded in [`RawResponseFormat`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawResponseJsonSchema {
    pub name: String,
    pub strict: Option<bool>,
    pub schema: serde_json::Value,
}

/// Typed structured-response description for one completion call.
///
/// ```rust
/// use agents::response::TypedResponse;
/// use schemars::JsonSchema;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
/// struct Answer {
///     text: String,
/// }
///
/// let format = TypedResponse::<Answer>::new("answer");
/// let raw = format.to_raw_response_format();
///
/// assert_eq!(raw.r#type, "json_schema");
/// assert!(raw.json_schema.is_some());
/// ```
#[derive(Clone)]
pub struct TypedResponse<R> {
    name: String,
    strict: bool,
    schema: serde_json::Value,
    _phantom: PhantomData<R>,
}

impl<R> TypedResponse<R>
where
    R: JsonSchema,
{
    /// Builds a strict JSON Schema response format for `R`.
    pub fn new(name: impl Into<String>) -> Self {
        let schema = schemars::schema_for!(R);
        let schema_json = serde_json::to_value(&schema).unwrap_or_default();

        Self {
            name: name.into(),
            strict: true,
            schema: schema_json,
            _phantom: PhantomData,
        }
    }

    /// Controls whether providers should enforce the schema strictly when supported.
    pub fn with_strict(mut self, strict: bool) -> Self {
        self.strict = strict;
        self
    }

    /// Converts the typed schema into the raw provider-facing payload.
    pub fn to_raw_response_format(&self) -> RawResponseFormat {
        RawResponseFormat {
            r#type: "json_schema".to_string(),
            json_schema: Some(RawResponseJsonSchema {
                name: self.name.clone(),
                strict: Some(self.strict),
                schema: self.schema.clone(),
            }),
        }
    }
}

impl<R> fmt::Debug for TypedResponse<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypedResponse")
            .field("name", &self.name)
            .field("strict", &self.strict)
            .finish()
    }
}
