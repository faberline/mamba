//! Tests for the fixed, independent literal-format mapping controls.
use super::*;
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{Expr, ExprCall, Stmt},
    PythonTarget,
};
use serde_json::Value;

const MAPPING_VECTORS: &str = r#####"{
  "schema": "literal-format.pure-mapping-control-proposal.v1",
  "cases": [
    {
      "id": "automatic-single",
      "format_value": "{}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "automatic-two",
      "format_value": "{} {}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        },
        {
          "argument_index": 1,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "ordinal-order",
      "format_value": "{1} {0}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 1,
          "converted": false
        },
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "ordinal-repeat",
      "format_value": "{0} {0}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        },
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "leading-zero",
      "format_value": "{00}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "named",
      "format_value": "{value}",
      "positional_argument_count": 0,
      "keyword_names": [
        "value"
      ],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "named-plus-auto",
      "format_value": "{name} {}",
      "positional_argument_count": 1,
      "keyword_names": [
        "name"
      ],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 1,
          "converted": false
        },
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "auto-plus-name",
      "format_value": "{} {name}",
      "positional_argument_count": 1,
      "keyword_names": [
        "name"
      ],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        },
        {
          "argument_index": 1,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "ordinal-plus-name",
      "format_value": "{0} {name}",
      "positional_argument_count": 1,
      "keyword_names": [
        "name"
      ],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        },
        {
          "argument_index": 1,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "named-repeat",
      "format_value": "{name} {name}",
      "positional_argument_count": 0,
      "keyword_names": [
        "name"
      ],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        },
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "escaped-and-field",
      "format_value": "{{}} {}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "unicode-literal-text",
      "format_value": "\u00e9:{}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "empty-spec",
      "format_value": "{:}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "repr-conversion",
      "format_value": "{!r}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "str-conversion",
      "format_value": "{!s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "ascii-conversion",
      "format_value": "{!a}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "conversion-empty-spec",
      "format_value": "{0!r:}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "mixed-conversion",
      "format_value": "{!r} {}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        },
        {
          "argument_index": 1,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "no-fields",
      "format_value": "plain",
      "positional_argument_count": 0,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "escaped-only",
      "format_value": "{{}}",
      "positional_argument_count": 0,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "missing-position",
      "format_value": "{1}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "missing-key",
      "format_value": "{name}",
      "positional_argument_count": 0,
      "keyword_names": [
        "other"
      ],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "unused-position",
      "format_value": "{}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "unused-key",
      "format_value": "{}",
      "positional_argument_count": 1,
      "keyword_names": [
        "other"
      ],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "mixed-numbering",
      "format_value": "{} {0}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "late-unmatched",
      "format_value": "{} {",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "bare-close",
      "format_value": "}",
      "positional_argument_count": 0,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "unmatched-open",
      "format_value": "{",
      "positional_argument_count": 0,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "bad-conversion",
      "format_value": "{!x}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "missing-conversion",
      "format_value": "{!}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "double-conversion",
      "format_value": "{!rr}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "typed-spec",
      "format_value": "{:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "flags-spec",
      "format_value": "{:>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ],
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "dynamic-spec",
      "format_value": "{0:{1}}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "custom-spec",
      "format_value": "{:date}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "attribute-field",
      "format_value": "{0.name}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "index-field",
      "format_value": "{0[0]}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "unicode-key",
      "format_value": "{\u00e9}",
      "positional_argument_count": 0,
      "keyword_names": [
        "\u00e9"
      ],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "unicode-number",
      "format_value": "{\u0661}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "too-large-ordinal",
      "format_value": "{999999999999999999999999999}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "duplicate-keyword",
      "format_value": "{name}",
      "positional_argument_count": 0,
      "keyword_names": [
        "name",
        "name"
      ],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "dstar-keyword",
      "format_value": "{name}",
      "positional_argument_count": 0,
      "keyword_names": [
        null
      ],
      "has_starred_positional": false,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    },
    {
      "id": "starred-positional",
      "format_value": "{}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": true,
      "expected_mapping": null,
      "expected_rust_execution": "UNRUN",
      "meaning": "Only pure complete field/argument mapping; no diagnostic or original exit expectation."
    }
  ],
  "count": 43,
  "actual_Rust_mapping_AST_tests": "UNRUN",
  "root_execution_required": true,
  "default_paths": "Unsupported mappings must leave the prior visitor path intact. Fully converted/empty mappings need no new visitor stage.",
  "whole_acceptance": false
}
"#####;

fn with_call<R>(source: &str, test: impl FnOnce(&ExprCall) -> R) -> R {
    let document = quality_syntax::parse(source, PythonTarget::Py314, false)
        .expect("Ruff parser accepts the generated call fixture");
    assert!(
        document.is_valid(),
        "call fixture syntax: {:?}",
        document.issues
    );
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        panic!("one expression statement is required");
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        panic!("the expression must be an actual Ruff ExprCall");
    };
    test(call)
}

fn vector_source(case: &Value) -> String {
    let template = case["format_value"].as_str().expect("fixed template");
    let positional = case["positional_argument_count"]
        .as_u64()
        .expect("fixed positional count") as usize;
    let starred = case["has_starred_positional"]
        .as_bool()
        .expect("fixed star flag");
    let mut arguments = Vec::new();
    for index in 0..positional {
        arguments.push(if starred && index == 0 {
            format!("*pos{index}")
        } else {
            format!("pos{index}")
        });
    }
    for (index, name) in case["keyword_names"]
        .as_array()
        .expect("fixed keyword list")
        .iter()
        .enumerate()
    {
        if name.is_null() {
            arguments.push(format!("**kwargs{index}"));
        } else {
            arguments.push(format!(
                "{}=kw{index}",
                name.as_str().expect("fixed keyword name")
            ));
        }
    }
    format!(
        "{}.format({})",
        serde_json::to_string(template).expect("Python-compatible string literal"),
        arguments.join(", ")
    )
}

fn assert_vector_mapping(call: &ExprCall, case: &Value) {
    let template = case["format_value"].as_str().expect("fixed template");
    let expected = case["expected_mapping"].as_array();
    let Some(fields) = parse_fields(template) else {
        assert!(
            expected.is_none(),
            "{}: malformed template cannot map",
            case["id"]
        );
        return;
    };
    let actual = map_fields(call, fields);
    let Some(expected) = expected else {
        assert!(
            actual.is_none(),
            "{}: incomplete mapping must reject",
            case["id"]
        );
        return;
    };
    let actual = actual.expect("complete fixed mapping");
    assert_eq!(actual.len(), expected.len(), "{}", case["id"]);
    for (replacement, expected) in actual.iter().zip(expected) {
        let index = expected["argument_index"]
            .as_u64()
            .expect("fixed argument index") as usize;
        let original = if index < call.arguments.args.len() {
            &call.arguments.args[index]
        } else {
            &call.arguments.keywords[index - call.arguments.args.len()].value
        };
        assert!(
            std::ptr::eq(replacement.expression, original),
            "{}: replacement must point at its original Ruff argument Expr",
            case["id"]
        );
        assert_eq!(
            replacement.converted,
            expected["converted"]
                .as_bool()
                .expect("fixed conversion flag"),
            "{}",
            case["id"]
        );
    }
}

#[test]
fn fixed_43_mapping_vectors_match_real_ruff_ast_arguments() {
    let catalog: Value = serde_json::from_str(MAPPING_VECTORS).expect("fixed vector JSON");
    assert_eq!(catalog["count"].as_u64(), Some(43));
    let cases = catalog["cases"].as_array().expect("fixed vector rows");
    assert_eq!(cases.len(), 43);
    for case in cases {
        let source = vector_source(case);
        if case["id"] == "duplicate-keyword" {
            // The source parser rejects repeated names on some Ruff revisions.
            // Clone its actual ExprCall, then form the invalid AST input directly.
            let template = case["format_value"].as_str().expect("fixed template");
            let source = format!(
                "{}.format(name=left, other=right)",
                serde_json::to_string(template).expect("Python-compatible string literal")
            );
            with_call(&source, |parsed| {
                assert_eq!(parsed.arguments.keywords.len(), 2);
                let mut duplicate = parsed.clone();
                duplicate.arguments.keywords[1].arg = duplicate.arguments.keywords[0].arg.clone();
                assert_vector_mapping(&duplicate, case);
            });
        } else {
            with_call(&source, |call| assert_vector_mapping(call, case));
        }
    }
}

#[test]
fn parsed_starred_and_double_starred_arguments_are_rejected() {
    with_call(r#""{}".format(*values)"#, |call| {
        assert!(matches!(
            call.arguments.args.first(),
            Some(Expr::Starred(_))
        ));
        assert!(map_fields(call, parse_fields("{}").unwrap()).is_none());
    });
    with_call(r#""{name}".format(**values)"#, |call| {
        assert!(call
            .arguments
            .keywords
            .iter()
            .any(|keyword| keyword.arg.is_none()));
        assert!(map_fields(call, parse_fields("{name}").unwrap()).is_none());
    });
}

#[test]
fn empty_complete_mapping_differs_from_incomplete_or_malformed_mapping() {
    with_call(r#""plain".format()"#, |call| {
        let fields = parse_fields("plain").expect("plain text parses completely");
        assert!(map_fields(call, fields)
            .expect("empty mapping is complete")
            .is_empty());
    });
    with_call(r#""{{}}".format()"#, |call| {
        let fields = parse_fields("{{}}").expect("escaped braces parse completely");
        assert!(map_fields(call, fields)
            .expect("escaped-only mapping is complete")
            .is_empty());
    });
    with_call(r#""{}".format()"#, |call| {
        let fields = parse_fields("{}").expect("field syntax parses");
        assert!(
            map_fields(call, fields).is_none(),
            "missing argument is incomplete"
        );
    });
    assert!(
        parse_fields("{} {").is_none(),
        "late unmatched open rejects the full template"
    );
    assert!(
        parse_fields("}").is_none(),
        "bare close rejects the full template"
    );
}

#[test]
fn named_and_positional_fields_retain_original_ast_argument_pointers() {
    with_call(r#""{name} {0}".format(positional, name=keyword)"#, |call| {
        let Expr::Attribute(attribute) = call.func.as_ref() else {
            panic!("format receiver must be a real Ruff attribute expression");
        };
        assert!(matches!(attribute.value.as_ref(), Expr::StringLiteral(_)));
        let replacements = map_fields(call, parse_fields("{name} {0}").unwrap())
            .expect("named and positional mapping is complete");
        assert_eq!(replacements.len(), 2);
        assert!(std::ptr::eq(
            replacements[0].expression,
            &call.arguments.keywords[0].value
        ));
        assert!(std::ptr::eq(
            replacements[1].expression,
            &call.arguments.args[0]
        ));
    });
}

#[test]
fn duplicate_keyword_names_reject_a_real_ruff_call_node() {
    with_call(r#""{name}".format(name=left, other=right)"#, |parsed| {
        let mut duplicate = parsed.clone();
        duplicate.arguments.keywords[1].arg = duplicate.arguments.keywords[0].arg.clone();
        assert!(map_fields(&duplicate, parse_fields("{name}").unwrap()).is_none());
    });
}
