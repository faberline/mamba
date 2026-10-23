//! Type-comment safety decisions compared to three isolated CPython oracles.
use mamba_language::application::quality_syntax::{parse, PythonTarget};
use serde_json::Value;

#[test]
fn preserves_cpython_type_comment_ownership() {
    let cases: Vec<Value> = serde_json::from_str(super::type_comments_data::CASES).unwrap();
    let mut failures = Vec::new();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        for (index, target) in [
            PythonTarget::Py312,
            PythonTarget::Py313,
            PythonTarget::Py314,
        ]
        .into_iter()
        .enumerate()
        {
            let expected = &case["expected"][index];
            if expected.get("error").is_some() {
                continue;
            }
            let document = parse(source, target, false).unwrap();
            assert!(document.is_valid(), "{:?}", document.issues);
            let actual =
                serde_json::to_value(super::super::type_comments::fingerprint(&document)).unwrap();
            if actual != expected["values"] {
                failures.push(
                    serde_json::json!({"source":source,"target":format!("{target:?}"),
                    "expected":expected,"actual":actual}),
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}
