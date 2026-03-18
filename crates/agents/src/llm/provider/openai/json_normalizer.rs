use serde_json::{Map, Value};

pub(super) fn normalize_openai_schema(schema: Value) -> Value {
    match schema {
        Value::Object(mut map) => {
            for value in map.values_mut() {
                let normalized = normalize_openai_schema(std::mem::take(value));
                *value = normalized;
            }

            if map.get("type").and_then(Value::as_str) == Some("object") {
                close_openai_object_schema(&mut map);
            }

            Value::Object(map)
        }
        Value::Array(values) => {
            Value::Array(values.into_iter().map(normalize_openai_schema).collect())
        }
        other => other,
    }
}

fn close_openai_object_schema(map: &mut Map<String, Value>) {
    map.insert("additionalProperties".to_string(), Value::Bool(false));

    let required = map
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| {
            Value::Array(
                properties
                    .keys()
                    .cloned()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_else(|| Value::Array(Vec::new()));
    map.insert("required".to_string(), required);
}

#[cfg(test)]
mod tests {
    use super::normalize_openai_schema;
    use serde_json::json;

    fn assert_required_keys(value: &serde_json::Value, expected: &[&str]) {
        let mut actual = value
            .as_array()
            .expect("required to be an array")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<Vec<_>>();
        actual.sort_unstable();

        let mut expected = expected.to_vec();
        expected.sort_unstable();

        assert_eq!(actual, expected);
    }

    #[test]
    fn closes_nested_object_schemas_inside_defs() {
        let schema = json!({
            "type": "object",
            "properties": {
                "evidence": { "$ref": "#/$defs/Evidence" }
            },
            "$defs": {
                "Evidence": {
                    "type": "object",
                    "properties": {
                        "strengths": { "type": "array", "items": { "type": "string" } },
                        "concerns": { "type": "array", "items": { "type": "string" } }
                    }
                }
            }
        });

        let normalized = normalize_openai_schema(schema);
        assert_eq!(normalized["additionalProperties"], json!(false));
        assert_required_keys(&normalized["required"], &["evidence"]);
        assert_eq!(
            normalized["$defs"]["Evidence"]["additionalProperties"],
            json!(false)
        );
        assert_required_keys(
            &normalized["$defs"]["Evidence"]["required"],
            &["strengths", "concerns"],
        );
    }

    #[test]
    fn closes_object_items_inside_arrays() {
        let schema = json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                }
            }
        });

        let normalized = normalize_openai_schema(schema);
        assert_eq!(normalized["items"]["additionalProperties"], json!(false));
        assert_required_keys(&normalized["items"]["required"], &["name"]);
    }

    #[test]
    fn leaves_scalar_schema_unchanged() {
        let schema = json!({ "type": "string" });
        let normalized = normalize_openai_schema(schema.clone());
        assert_eq!(normalized, schema);
    }
}
