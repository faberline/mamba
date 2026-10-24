use serde_json::Value;

pub(super) fn validate_json_schema_value(
    value: &Value,
    schema: &Value,
    path: &str,
    errors: &mut Vec<String>,
) {
    if validate_any_of_schema(value, schema, path, errors) {
        return;
    }

    match schema_type(schema) {
        Some("object") => validate_object_schema(value, schema, path, errors),
        Some("array") => validate_array_schema(value, schema, path, errors),
        Some("string") => validate_string_schema(value, schema, path, errors),
        Some("integer") => validate_integer_schema(value, schema, path, errors),
        Some("number") => validate_number_schema(value, schema, path, errors),
        Some("boolean") => {
            if !value.is_boolean() {
                errors.push(format!(
                    "{path}: expected boolean, got {}",
                    json_type(value)
                ));
            }
        }
        Some("null") => {
            if !value.is_null() {
                errors.push(format!("{path}: expected null, got {}", json_type(value)));
            }
        }
        _ => {}
    }
}

fn validate_any_of_schema(
    value: &Value,
    schema: &Value,
    path: &str,
    errors: &mut Vec<String>,
) -> bool {
    let Some(any_of) = schema.get("anyOf").and_then(Value::as_array) else {
        return false;
    };

    if any_of.is_empty() {
        errors.push(format!("{path}: anyOf requires at least one schema"));
        return true;
    }

    let mut branch_messages = Vec::new();
    for branch in any_of {
        let mut branch_errors = Vec::new();
        validate_json_schema_value(value, branch, path, &mut branch_errors);
        if branch_errors.is_empty() {
            return true;
        }
        branch_messages.extend(branch_errors);
    }

    errors.push(format!(
        "{path}: did not match anyOf ({})",
        branch_messages.join("; ")
    ));
    true
}

fn validate_object_schema(value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(object) = value.as_object() else {
        errors.push(format!("{path}: expected object, got {}", json_type(value)));
        return;
    };

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !object.contains_key(field) {
                errors.push(format!("{path}.{field}: missing required field"));
            }
        }
    }

    let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
        return;
    };
    for (field, field_schema) in properties {
        if let Some(field_value) = object.get(field) {
            validate_json_schema_value(
                field_value,
                field_schema,
                &format!("{path}.{field}"),
                errors,
            );
        }
    }
}

fn validate_array_schema(value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(items) = value.as_array() else {
        errors.push(format!("{path}: expected array, got {}", json_type(value)));
        return;
    };
    if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
        if (items.len() as u64) < min {
            errors.push(format!("{path}: expected at least {min} items"));
        }
    }
    if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
        if (items.len() as u64) > max {
            errors.push(format!("{path}: expected at most {max} items"));
        }
    }
    if schema
        .get("uniqueItems")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        for (idx, item) in items.iter().enumerate() {
            if items[..idx].iter().any(|prior| prior == item) {
                errors.push(format!("{path}: expected unique items"));
                break;
            }
        }
    }
    if let Some(item_schema) = schema.get("items") {
        for (idx, item) in items.iter().enumerate() {
            validate_json_schema_value(item, item_schema, &format!("{path}[{idx}]"), errors);
        }
    }
}

fn validate_string_schema(value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(text) = value.as_str() else {
        errors.push(format!("{path}: expected string, got {}", json_type(value)));
        return;
    };
    if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
        if (text.chars().count() as u64) < min {
            errors.push(format!("{path}: expected at least {min} characters"));
        }
    }
    if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
        if (text.chars().count() as u64) > max {
            errors.push(format!("{path}: expected at most {max} characters"));
        }
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        match regex::Regex::new(pattern) {
            Ok(regex) => {
                if !regex.is_match(text) {
                    errors.push(format!("{path}: expected to match pattern {pattern:?}"));
                }
            }
            Err(err) => errors.push(format!("{path}: invalid pattern {pattern:?}: {err}")),
        }
    }
}

fn validate_integer_schema(value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(number) = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|v| i64::try_from(v).ok()))
    else {
        errors.push(format!(
            "{path}: expected integer, got {}",
            json_type(value)
        ));
        return;
    };
    validate_numeric_schema(number as f64, schema, path, errors);
}

fn validate_number_schema(value: &Value, schema: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(number) = value.as_f64() else {
        errors.push(format!("{path}: expected number, got {}", json_type(value)));
        return;
    };
    validate_numeric_schema(number, schema, path, errors);
}

fn validate_numeric_schema(number: f64, schema: &Value, path: &str, errors: &mut Vec<String>) {
    if let Some(min) = schema_number(schema, "minimum") {
        if number < min {
            errors.push(format!("{path}: expected >= {min}"));
        }
    }
    if let Some(max) = schema_number(schema, "maximum") {
        if number > max {
            errors.push(format!("{path}: expected <= {max}"));
        }
    }
    if let Some(min) = schema_number(schema, "exclusiveMinimum") {
        if number <= min {
            errors.push(format!("{path}: expected > {min}"));
        }
    } else if schema_bool(schema, "exclusiveMinimum") {
        if let Some(min) = schema_number(schema, "minimum") {
            if number <= min {
                errors.push(format!("{path}: expected > {min}"));
            }
        }
    }
    if let Some(max) = schema_number(schema, "exclusiveMaximum") {
        if number >= max {
            errors.push(format!("{path}: expected < {max}"));
        }
    } else if schema_bool(schema, "exclusiveMaximum") {
        if let Some(max) = schema_number(schema, "maximum") {
            if number >= max {
                errors.push(format!("{path}: expected < {max}"));
            }
        }
    }
    if let Some(multiple) = schema_number(schema, "multipleOf") {
        if multiple <= 0.0 || !is_multiple_of(number, multiple) {
            errors.push(format!("{path}: expected multiple of {multiple}"));
        }
    }
}

fn schema_number(schema: &Value, key: &str) -> Option<f64> {
    schema.get(key).and_then(Value::as_f64)
}

fn schema_bool(schema: &Value, key: &str) -> bool {
    schema.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn is_multiple_of(number: f64, multiple: f64) -> bool {
    if !number.is_finite() || !multiple.is_finite() {
        return false;
    }
    let quotient = number / multiple;
    (quotient - quotient.round()).abs() <= f64::EPSILON * quotient.abs().max(1.0) * 16.0
}

fn schema_type(schema: &Value) -> Option<&str> {
    schema.get("type").and_then(Value::as_str)
}

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
