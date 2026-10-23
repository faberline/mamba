// Frozen from mypy 2.4.0 / Pydantic 2.13.5 three-version reference output.
use mamba_quality::{run, QualityOperation, QualityRequest};
use serde_json::{json, Value};
use std::fs;

fn compare(source: &str, expected: &str) {
    for version in ["3.12", "3.13", "3.14"] {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("main.py");
        fs::write(
            project.path().join("pyproject.toml"),
            format!("[project]\nrequires-python = '>={version}'\n"),
        )
        .unwrap();
        fs::write(&path, source).unwrap();
        let report = run(&QualityRequest::new(QualityOperation::Check, vec![path]));
        assert_eq!(report.exit_code, 2);
        assert!(!report.analysis_completed);
        let tool = report
            .tools
            .iter()
            .find(|tool| tool.tool.name() == "mypy")
            .unwrap();
        let actual: Vec<_> = tool
            .diagnostics
            .iter()
            .map(|d| {
                json!({
                    "rule": d.rule, "message": d.message, "hint": d.hint,
                    "path": "main.py", "severity": d.severity,
                    "location": {"line": d.location.line, "column": d.location.column,
                        "end_line": d.location.end_line, "end_column": d.location.end_column},
                })
            })
            .collect();
        assert_eq!(
            Value::Array(actual),
            serde_json::from_str::<Value>(expected).unwrap(),
            "Python {version}"
        );
        assert_eq!(tool.reference_exit_code, None);
    }
}

#[test]
fn pydantic_fields_match_the_reference_diagnostics() {
    compare(
        r###"from pydantic import BaseModel, ConfigDict, Field

class Person(BaseModel):
    name: str
    age: int = 0
    nick: str = Field(alias="nickname")
    code: int = Field(default=1, frozen=True)

class Locked(Person):
    model_config = ConfigDict(frozen=True)

Person(name=3, nick="N", extra=1)
p = Person(name="A", nickname="N")
p.code = 2
locked = Locked(name="A", nickname="N")
locked.age = 3

class Dynamic(BaseModel):
    value: int = Field(alias=str(1))

class Defaults(BaseModel):
    count: int = Field(default="wrong")
    missing = 3
"###,
        r###"[
  {
    "rule": "call-arg",
    "message": "Unexpected keyword argument \"nick\" for \"Person\"",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 12,
      "column": 1,
      "end_line": 12,
      "end_column": 33
    }
  },
  {
    "rule": "call-arg",
    "message": "Unexpected keyword argument \"extra\" for \"Person\"",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 12,
      "column": 1,
      "end_line": 12,
      "end_column": 33
    }
  },
  {
    "rule": "arg-type",
    "message": "Argument \"name\" to \"Person\" has incompatible type \"int\"; expected \"str\"",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 12,
      "column": 13,
      "end_line": 12,
      "end_column": 13
    }
  },
  {
    "rule": "misc",
    "message": "Property \"code\" defined in \"Person\" is read-only",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 14,
      "column": 1,
      "end_line": 14,
      "end_column": 6
    }
  },
  {
    "rule": "misc",
    "message": "Property \"age\" defined in \"Locked\" is read-only",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 16,
      "column": 1,
      "end_line": 16,
      "end_column": 10
    }
  },
  {
    "rule": "pydantic-alias",
    "message": "Required dynamic aliases disallowed",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 19,
      "column": 5,
      "end_line": 19,
      "end_column": 36
    }
  },
  {
    "rule": "assignment",
    "message": "Incompatible types in assignment (expression has type \"str\", variable has type \"int\")",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 22,
      "column": 18,
      "end_line": 22,
      "end_column": 39
    }
  },
  {
    "rule": "pydantic-field",
    "message": "Untyped fields disallowed",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 23,
      "column": 5,
      "end_line": 23,
      "end_column": 15
    }
  }
]"###,
    );
}

#[test]
fn ignore_comments_match_the_reference_diagnostics() {
    compare(
        r###"MESSAGE = "# mypy: ignore-errors"
VALUE: int = "# type: ignore[assignment]"
COUNT: int = "wrong"  # type: ignore[assignment]
UNUSED: int = 1  # type: ignore[assignment]
def missing(x):  # type: ignore[no-untyped-def]
    return x
"###,
        r###"[
  {
    "rule": "assignment",
    "message": "Incompatible types in assignment (expression has type \"str\", variable has type \"int\")",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 2,
      "column": 14,
      "end_line": 2,
      "end_column": 41
    }
  },
  {
    "rule": "unused-ignore",
    "message": "Unused \"type: ignore\" comment",
    "hint": null,
    "path": "main.py",
    "severity": "error",
    "location": {
      "line": 4,
      "column": 0,
      "end_line": 4,
      "end_column": -1
    }
  }
]"###,
    );
}
