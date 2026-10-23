//! Original notes without an error code retain a JSON null rule.
use serde::{Deserialize, Deserializer, Serializer};

pub(super) fn serialize<S: Serializer>(rule: &str, serializer: S) -> Result<S::Ok, S::Error> {
    if rule.is_empty() {
        serializer.serialize_none()
    } else {
        serializer.serialize_str(rule)
    }
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use crate::domain::model::*;
    #[test]
    fn rule_codes_and_uncoded_notes_round_trip() {
        for (rule, expected) in [
            ("misc", serde_json::json!("misc")),
            ("", serde_json::Value::Null),
        ] {
            let diagnostic = QualityDiagnostic {
                tool: QualityTool::Mypy,
                rule: rule.into(),
                message: "message".into(),
                hint: None,
                path: "sample.py".into(),
                severity: "note".into(),
                fix: None,
                location: QualityLocation {
                    line: 1,
                    column: 1,
                    end_line: None,
                    end_column: None,
                    offset: None,
                    end_offset: None,
                },
            };
            let encoded = serde_json::to_value(&diagnostic).unwrap();
            assert_eq!(encoded["rule"], expected);
            let decoded: QualityDiagnostic = serde_json::from_value(encoded).unwrap();
            assert_eq!(decoded.rule, rule);
        }
    }
}
