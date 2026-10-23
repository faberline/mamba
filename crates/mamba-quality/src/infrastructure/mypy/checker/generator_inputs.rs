//! A contextual iterable checks generator items before its enclosing argument.
use super::*;

pub(super) enum CandidateFailure {
    Rejected,
    Unknown(ruff_text_size::TextRange, &'static str),
    RecoveryCandidates(Vec<RecoveredCandidate>),
}

pub(super) struct RecoveredCandidate {
    pub(super) signature: std::sync::Arc<super::super::signatures::Signature>,
    pub(super) value: CheckedValue,
    pub(super) types: types::ObservedExpressionTypes,
    pub(super) lease: Option<RecoveryCandidateLease>,
}

// A filtered call attempt is distinct from a committed partial origin error.
// Both retain actual results, and neither can prove an error-free match.
#[derive(Clone)]
pub(super) enum RecoveryOrigin {
    PartialRead(partial_sets::PartialReadRecovery),
    UntypedCall(UntypedCallRecovery),
}

#[derive(Clone)]
pub(super) struct UntypedCallRecovery {
    pub(super) project: usize,
    pub(super) path: std::path::PathBuf,
    pub(super) sha256: String,
    pub(super) root_call: usize,
    pub(super) call: usize,
    pub(super) range: ruff_text_size::TextRange,
    pub(super) binding: (
        String,
        String,
        ruff_text_size::TextSize,
        types::Type,
        std::sync::Arc<super::super::signatures::Signature>,
    ),
    pub(super) message: String,
}

pub(super) struct RecoveryCandidateLease {
    pub(super) project: usize,
    pub(super) path: std::path::PathBuf,
    pub(super) sha256: String,
    pub(super) root_call: usize,
    pub(super) call: usize,
    pub(super) owner: (usize, scopes::Kind, Option<usize>),
}

#[derive(Clone)]
pub(super) enum CheckedValue {
    Clean(types::Type),
    Recovered {
        actual: types::Type,
        origins: Vec<RecoveryOrigin>,
    },
}

impl CheckedValue {
    pub(super) fn actual(&self) -> types::Type {
        match self {
            Self::Clean(actual) | Self::Recovered { actual, .. } => *actual,
        }
    }

    pub(super) fn is_recovered(&self) -> bool {
        matches!(self, Self::Recovered { .. })
    }

    pub(super) fn has_untyped_origin(&self) -> bool {
        matches!(self, Self::Recovered { origins, .. }
            if origins.iter().any(|origin| matches!(origin, RecoveryOrigin::UntypedCall(_))))
    }

    pub(super) fn with_origin(self, origin: RecoveryOrigin) -> Self {
        let actual = self.actual();
        let mut origins = match self {
            Self::Clean(_) => Vec::new(),
            Self::Recovered { origins, .. } => origins,
        };
        origins.push(origin);
        Self::Recovered { actual, origins }
    }

    pub(super) fn combine(actual: types::Type, children: impl IntoIterator<Item = Self>) -> Self {
        let origins: Vec<_> = children
            .into_iter()
            .flat_map(|value| match value {
                Self::Clean(_) => Vec::new(),
                Self::Recovered { origins, .. } => origins,
            })
            .collect();
        if origins.is_empty() {
            Self::Clean(actual)
        } else {
            Self::Recovered { actual, origins }
        }
    }
}

pub(super) type CheckedExpression = Result<CheckedValue, CandidateFailure>;

pub(super) fn unknown_expression(
    expression: &impl Ranged,
    reason: &'static str,
) -> CheckedExpression {
    Err(CandidateFailure::Unknown(expression.range(), reason))
}

impl Checker<'_> {
    pub(super) fn recovery_origins_are_current(&self, origins: &[RecoveryOrigin]) -> bool {
        !origins.is_empty()
            && origins.iter().all(|origin| match origin {
                RecoveryOrigin::PartialRead(receipt) => {
                    self.partial_read_recovery_is_current(receipt)
                }
                RecoveryOrigin::UntypedCall(receipt) => {
                    receipt.project == self.source.project
                        && receipt.path == self.source.path
                        && receipt.sha256 == self.source.sha256
                        && self.checked_source_call == Some(receipt.root_call)
                        && receipt.call != 0
                        && !receipt.range.is_empty()
                        && receipt.range.end().to_usize() <= self.source.bytes.len()
                        && !receipt.message.is_empty()
                        && self.scopes.candidate_source_callable_is_current(
                            &receipt.binding,
                            &self.environment(),
                        )
                }
            })
    }

    pub(super) fn check_candidate_generator(
        &mut self,
        generator: &quality_syntax::python_ast::ExprGenerator,
        expected: Option<types::Type>,
    ) -> CheckedExpression {
        use types::{Container, Node, Type};
        let provisional_context = self.provisional_generator_context;
        if generator.generators.is_empty()
            || generator
                .generators
                .iter()
                .any(|clause| clause.is_async || !clause.ifs.is_empty())
        {
            return unknown_expression(generator, "async or filtered generator");
        }
        let expected_item = match expected {
            Some(expected) => match self.context.iterable_item(expected) {
                Some(item) => Some(item),
                None => return unknown_expression(generator, "unknown iterable context"),
            },
            None => None,
        };
        self.check_candidate_comprehension(&generator.generators, |checker, mut checked_values| {
            let item = checker.check_candidate_expression(&generator.elt, expected_item)?;
            let actual_item = item.actual();
            checked_values.push(item);
            if let Some(expected) = expected_item {
                if !checker.context.accepts(expected, actual_item) {
                    if provisional_context {
                        return unknown_expression(
                            generator,
                            "rejected provisional generator context",
                        );
                    }
                    checker.emit(
                        "misc",
                        format!(
                            "Generator has incompatible item type \"{}\"; expected \"{}\"",
                            checker.context.name(actual_item),
                            checker.context.name(expected)
                        ),
                        generator.elt.as_ref(),
                    );
                    if !checked_values.iter().any(CheckedValue::is_recovered) {
                        return Err(CandidateFailure::Rejected);
                    }
                }
            }
            let actual = checker.context.intern(Node::Container(
                Container::Generator,
                vec![actual_item, Type::None, Type::None],
            ));
            Ok(CheckedValue::combine(actual, checked_values))
        })
    }

    pub(super) fn check_candidate_comprehension(
        &mut self,
        generators: &[quality_syntax::python_ast::Comprehension],
        body: impl FnOnce(&mut Self, Vec<CheckedValue>) -> CheckedExpression,
    ) -> CheckedExpression {
        // Only the first iterable executes in the enclosing owner.
        let first = self.check_candidate_expression(&generators[0].iter, None)?;
        let first_actual = first.actual();
        let mut checked_values = vec![first];
        let initial = self.environment();
        let previous_flow = std::mem::take(&mut self.flow);
        let previous_owner = self.checked_expression_owner;
        let previous_types = self.checked_expression_types.take();
        let previous_expected = self.expected_value.take();
        let environment = self.scopes.enter_candidate_generator(generators, &initial);
        self.flow.narrowed.extend(environment);
        self.checked_expression_owner = Some(self.scopes.try_input_scope());
        let local_types = std::sync::Arc::new(std::sync::Mutex::new(Default::default()));
        self.checked_expression_types = Some(local_types.clone());
        let context = self.context.clone();
        // No early return can bypass the lexical frame's restoration below.
        let checked = context.with_checked_expression_types(&local_types, || {
            for (index, clause) in generators.iter().enumerate() {
                let iterable = if index == 0 {
                    first_actual
                } else {
                    let value = self.check_candidate_expression(&clause.iter, None)?;
                    let actual = value.actual();
                    checked_values.push(value);
                    actual
                };
                let Some(item) = self
                    .context
                    .checked_candidate_iterable_item(iterable, self.python_target)
                else {
                    return unknown_expression(&clause.iter, "unknown generator iterable item");
                };
                let Some(bindings) = self
                    .scopes
                    .bind_candidate_generator_target(&clause.target, item)
                else {
                    return unknown_expression(&clause.target, "unproved generator target binding");
                };
                for (name, ty) in bindings {
                    self.flow.narrowed.retain(|key, _| {
                        key != &name
                            && !key
                                .strip_prefix(&name)
                                .is_some_and(|tail| tail.starts_with('.') || tail.starts_with('['))
                    });
                    self.flow.narrowed.insert(name, ty);
                }
            }
            body(self, checked_values)
        });
        self.scopes.leave();
        self.flow = previous_flow;
        self.checked_expression_owner = previous_owner;
        self.checked_expression_types = previous_types;
        self.expected_value = previous_expected;
        checked
    }

    pub(super) fn infer_generator_expected(
        &mut self,
        expression: &Expr,
        expected: types::Type,
    ) -> Option<types::Type> {
        let Expr::Generator(generator) = expression else {
            return None;
        };
        let item = self.context.iterable_item(expected)?;
        let actual = self.infer(expression)?;
        let node = self.context.node(actual)?;
        let types::Node::Container(types::Container::Generator, arguments) = node.as_ref() else {
            return None;
        };
        if !self.context.accepts(item, arguments[0]) {
            self.emit(
                "misc",
                format!(
                    "Generator has incompatible item type \"{}\"; expected \"{}\"",
                    self.context.name(arguments[0]),
                    self.context.name(item)
                ),
                generator.elt.as_ref(),
            );
        }
        let mut arguments = arguments.clone();
        arguments[0] = item;
        Some(self.context.intern(types::Node::Container(
            types::Container::Generator,
            arguments,
        )))
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{model::CompletionStatus, ports::ProjectStore, source::PythonVersion};
    use crate::infrastructure::filesystem::Filesystem;
    use serde_json::{json, Value};
    use std::fs;

    fn diagnostics_for(source: &str) -> Vec<Value> {
        let root = tempfile::tempdir().unwrap();
        let root_path = fs::canonicalize(root.path()).unwrap();
        fs::write(
            root_path.join("pyproject.toml"),
            "[project]\nname = \"fixture\"\nversion = \"0.0.0\"\nrequires-python = \">=3.12\"\n",
        )
        .unwrap();
        let path = root_path.join("main.py");
        fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;
        let report = crate::infrastructure::mypy::check(&workspace);
        assert_eq!(report.completion, CompletionStatus::Incomplete);
        assert_eq!(report.reference_exit_code, None);
        assert_eq!(report.checked_files, 1);
        let diagnostics = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "rule": diagnostic.rule,
                    "message": diagnostic.message,
                    "hint": diagnostic.hint,
                    "path": diagnostic.path.strip_prefix(&root_path).unwrap(),
                    "severity": diagnostic.severity,
                    "location": {
                        "line": diagnostic.location.line,
                        "column": diagnostic.location.column,
                        "end_line": diagnostic.location.end_line,
                        "end_column": diagnostic.location.end_column,
                    },
                })
            })
            .collect();
        assert_eq!(fs::read(&path).unwrap(), source.as_bytes());
        diagnostics
    }

    #[test]
    fn generator_item_error_survives_an_untyped_source_call() {
        let untyped = r#"from typing import Iterator

def read(path):
    return [0]

def checked(path: str) -> Iterator[tuple[int, bool]]:
    return enumerate((1 for _ in read(path)))
"#;
        let typed = r#"from typing import Iterator

def read(path: str) -> list[int]:
    return [0]

def checked(path: str) -> Iterator[tuple[int, bool]]:
    return enumerate((1 for _ in read(path)))
"#;
        let item_error = json!({
            "rule": "misc",
            "message": "Generator has incompatible item type \"int\"; expected \"bool\"",
            "hint": null,
            "path": "main.py",
            "severity": "error",
            "location": {"line": 7, "column": 23, "end_line": 7, "end_column": 23},
        });
        assert_eq!(
            diagnostics_for(untyped),
            vec![
                json!({
                    "rule": "no-untyped-def",
                    "message": "Function is missing a type annotation",
                    "hint": null,
                    "path": "main.py",
                    "severity": "error",
                    "location": {"line": 3, "column": 1, "end_line": 4, "end_column": 14},
                }),
                item_error.clone(),
                json!({
                    "rule": "no-untyped-call",
                    "message": "Call to untyped function \"read\" in typed context",
                    "hint": null,
                    "path": "main.py",
                    "severity": "error",
                    "location": {"line": 7, "column": 34, "end_line": 7, "end_column": 43},
                }),
            ]
        );
        assert_eq!(diagnostics_for(typed), vec![item_error]);
    }

    #[test]
    fn nested_sum_recovers_untyped_read_result() {
        let source = r#"import json
from pathlib import Path

def read(path):
    return json.loads(path.read_text())

def checked(receipts: list[Path]) -> None:
    assert sum(sum(r["count"] for r in read(path)["results"]) for path in receipts[:2]) == 214
"#;
        assert_eq!(
            diagnostics_for(source),
            vec![
                json!({
                    "rule": "no-untyped-def",
                    "message": "Function is missing a type annotation",
                    "hint": null,
                    "path": "main.py",
                    "severity": "error",
                    "location": {"line": 4, "column": 1, "end_line": 5, "end_column": 39},
                }),
                json!({
                    "rule": "misc",
                    "message": "Generator has incompatible item type \"int\"; expected \"bool\"",
                    "hint": null,
                    "path": "main.py",
                    "severity": "error",
                    "location": {"line": 8, "column": 16, "end_line": 8, "end_column": 61},
                }),
                json!({
                    "rule": "no-untyped-call",
                    "message": "Call to untyped function \"read\" in typed context",
                    "hint": null,
                    "path": "main.py",
                    "severity": "error",
                    "location": {"line": 8, "column": 40, "end_line": 8, "end_column": 49},
                }),
            ]
        );
    }
}
