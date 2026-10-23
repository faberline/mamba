//! Independent flat-spec cap tests; no candidate-derived oracle expectations.
use super::*;
use mamba_language::application::quality_syntax::{
    self,
    python_ast::{Expr, ExprCall, Stmt},
    PythonTarget,
};
use serde_json::Value;

const FLAT_VECTORS: &str = r#####"{
  "schema": "literal-format-flat-spec-controls.v1",
  "spec_cases": [
    {
      "spec": "",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "s",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": ">8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "<8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "^8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "<",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "^",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "#>8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "0>8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "_^12s",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "🌟>8s",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "8.3s",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": ".0s",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": ".00",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": ">8.3",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "s<8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": ":>8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "!^8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": " >8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "8s",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": ".3",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "*>8",
      "expected_supported": true,
      "authority": "bounded flag-free string subset of original compile_new_format_re and bytes-warning no-type/s condition"
    },
    {
      "spec": "+8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "-8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": " 8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "#8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "z8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "08",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "0",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "00",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ",",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "_",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ">8,",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ">8_",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "=8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ">08",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "ab>8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ">8ss",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ">8x",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ".",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ".s",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": ".3d",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "8d",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "x",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "custom",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "{{}}",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "{width}",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "{width}.2",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "}",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "٨",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "\n>8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "\t>8",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    },
    {
      "spec": "~",
      "expected_supported": false,
      "authority": "explicit bounded-cap exclusion or malformed/custom/dynamic/typed grammar; not a clean verdict"
    }
  ],
  "mapping_cases": [
    {
      "id": "no-conversion-s",
      "format_value": "{:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ]
    },
    {
      "id": "no-conversion->8",
      "format_value": "{:>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ]
    },
    {
      "id": "no-conversion-0>8",
      "format_value": "{:0>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ]
    },
    {
      "id": "no-conversion-🌟^8.3s",
      "format_value": "{:🌟^8.3s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": false
        }
      ]
    },
    {
      "id": "!r-s",
      "format_value": "{!r:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!r->8",
      "format_value": "{!r:>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!r-0>8",
      "format_value": "{!r:0>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!r-🌟^8.3s",
      "format_value": "{!r:🌟^8.3s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!s-s",
      "format_value": "{!s:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!s->8",
      "format_value": "{!s:>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!s-0>8",
      "format_value": "{!s:0>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!s-🌟^8.3s",
      "format_value": "{!s:🌟^8.3s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!a-s",
      "format_value": "{!a:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!a->8",
      "format_value": "{!a:>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!a-0>8",
      "format_value": "{!a:0>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "!a-🌟^8.3s",
      "format_value": "{!a:🌟^8.3s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        }
      ]
    },
    {
      "id": "named-and-position-flat",
      "format_value": "{name:>8s} {0:.2}",
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
      ]
    },
    {
      "id": "mixed-converted-flat",
      "format_value": "{0!r:>8} {0:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": [
        {
          "argument_index": 0,
          "converted": true
        },
        {
          "argument_index": 0,
          "converted": false
        }
      ]
    },
    {
      "id": "nested-incomplete",
      "format_value": "{0:{1}}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null
    },
    {
      "id": "late-malformed-incomplete",
      "format_value": "{:s} {",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null
    },
    {
      "id": "mixed-numbering-incomplete",
      "format_value": "{:s} {0:>8}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null
    },
    {
      "id": "extra-flat-argument-incomplete",
      "format_value": "{:s}",
      "positional_argument_count": 2,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null
    },
    {
      "id": "missing-flat-argument-incomplete",
      "format_value": "{:>8}",
      "positional_argument_count": 0,
      "keyword_names": [],
      "has_starred_positional": false,
      "expected_mapping": null
    },
    {
      "id": "star-flat-incomplete",
      "format_value": "{:s}",
      "positional_argument_count": 1,
      "keyword_names": [],
      "has_starred_positional": true,
      "expected_mapping": null
    },
    {
      "id": "dstar-flat-incomplete",
      "format_value": "{name:s}",
      "positional_argument_count": 0,
      "keyword_names": [
        null
      ],
      "has_starred_positional": false,
      "expected_mapping": null
    },
    {
      "id": "accessor-flat-incomplete",
      "format_value": "{name.value:s}",
      "positional_argument_count": 0,
      "keyword_names": [
        "name"
      ],
      "has_starred_positional": false,
      "expected_mapping": null
    }
  ],
  "actual_Rust_execution": "UNRUN",
  "diagnostic_expectations": null,
  "whole_acceptance": false
}
"#####;

fn with_call(source: &str, test: impl FnOnce(&ExprCall)) {
    let document = quality_syntax::parse(source, PythonTarget::Py314, false)
        .expect("Rust parser accepts this independent mapping fixture");
    assert!(document.is_valid(), "fixture syntax: {:?}", document.issues);
    let [Stmt::Expr(statement)] = document.parsed.syntax().body.as_slice() else {
        panic!("exactly one expression statement is required");
    };
    let Expr::Call(call) = statement.value.as_ref() else {
        panic!("actual Ruff call is required");
    };
    test(call);
}

#[test]
fn flat_string_spec_matches_primary_rule_subset() {
    let data: Value = serde_json::from_str(FLAT_VECTORS).unwrap();
    let cases = data["spec_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    for case in cases {
        let spec = case["spec"].as_str().unwrap();
        assert_eq!(
            flat_string_spec(spec),
            case["expected_supported"].as_bool().unwrap(),
            "independent bounded spec: {spec:?}"
        );
    }
}

#[test]
fn flat_mapping_keeps_original_argument_identity_and_conversion() {
    let data: Value = serde_json::from_str(FLAT_VECTORS).unwrap();
    let cases = data["mapping_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 26);
    for case in cases {
        let template = case["format_value"].as_str().unwrap();
        let positional = case["positional_argument_count"].as_u64().unwrap() as usize;
        let starred = case["has_starred_positional"].as_bool().unwrap();
        let mut arguments: Vec<String> = (0..positional)
            .map(|index| {
                if starred && index == 0 {
                    format!("*pos{index}")
                } else {
                    format!("pos{index}")
                }
            })
            .collect();
        for (index, name) in case["keyword_names"].as_array().unwrap().iter().enumerate() {
            arguments.push(if let Some(name) = name.as_str() {
                format!("{name}=kw{index}")
            } else {
                format!("**kwargs{index}")
            });
        }
        let source = format!(
            "{}.format({})",
            serde_json::to_string(template).unwrap(),
            arguments.join(", ")
        );
        with_call(&source, |call| {
            let actual = parse_fields(template).and_then(|fields| map_fields(call, fields));
            let Some(expected) = case["expected_mapping"].as_array() else {
                assert!(actual.is_none(), "unsupported mapping: {}", case["id"]);
                return;
            };
            let actual = actual.expect("complete flat mapping");
            assert_eq!(actual.len(), expected.len(), "{}", case["id"]);
            for (actual, expected) in actual.iter().zip(expected) {
                let index = expected["argument_index"].as_u64().unwrap() as usize;
                let original = if index < call.arguments.args.len() {
                    &call.arguments.args[index]
                } else {
                    &call.arguments.keywords[index - call.arguments.args.len()].value
                };
                assert!(std::ptr::eq(actual.expression, original), "{}", case["id"]);
                assert_eq!(actual.converted, expected["converted"].as_bool().unwrap());
            }
        });
    }
}
