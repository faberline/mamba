//! Contextual comprehension errors use the inferred lexical element type.
use super::*;

impl Checker<'_> {
    pub(super) fn infer_comprehension_expected(
        &mut self,
        expression: &Expr,
        expected: Type,
    ) -> Option<Option<Type>> {
        match expression {
            Expr::ListComp(_) => self
                .infer_list_comprehension_expected(expression, expected)
                .map(Some),
            Expr::SetComp(_) => self
                .infer_set_comprehension_expected(expression, expected)
                .map(Some),
            Expr::DictComp(_) => self.infer_dict_comprehension_expected(expression, expected),
            _ => None,
        }
    }

    fn infer_dict_comprehension_expected(
        &mut self,
        expression: &Expr,
        expected: Type,
    ) -> Option<Option<Type>> {
        let Expr::DictComp(dictionary) = expression else {
            return None;
        };
        // Unpacked comprehensions need their separate mapping-call contract.
        let key = dictionary.key.as_deref()?;
        let expected_node = self.context.node(expected)?;
        let Node::Container(Container::Dict, expected_arguments) = expected_node.as_ref() else {
            return None;
        };
        if expected_arguments.len() != 2 {
            return None;
        }

        // Whole-expression overrides are authoritative. Do not recheck their
        // children under a new owner or recompute a supplied unknown.
        if let Some(actual) = self
            .context
            .observed_expression_type(expression)
            .or_else(|| self.observed_fstring_type(expression))
        {
            return Some(actual);
        }
        let environment = self
            .scopes
            .inference_environment(expression, self.environment());
        let Some((scope, environment)) = self
            .scopes
            .dictionary_comprehension_scope(dictionary, &environment)
        else {
            return Some(None);
        };
        // This is the existing lexical inference producer, not a speculative
        // checker snapshot. Move the enclosing state aside and restore it on
        // every return from the contextual key/value check below.
        let previous_scopes = std::mem::replace(&mut self.scopes, scope);
        let previous_flow = std::mem::take(&mut self.flow);
        let previous_inputs = std::mem::take(&mut self.try_inputs);
        let previous_owner = self.checked_expression_owner.take();
        let previous_types = self.checked_expression_types.take();
        let previous_contexts = std::mem::take(&mut self.checked_expression_contexts);
        let previous_expected = self.expected_value.take();
        self.flow.narrowed.extend(environment);
        let context = self.context.clone();
        let actual = context.without_observed(|| {
            let key_actual = self.infer_expected(key, expected_arguments[0])?;
            let value_actual = self.infer_expected(&dictionary.value, expected_arguments[1])?;
            for (kind, expected, actual, value) in [
                ("Key", expected_arguments[0], key_actual, key),
                (
                    "Value",
                    expected_arguments[1],
                    value_actual,
                    dictionary.value.as_ref(),
                ),
            ] {
                self.check_dictionary_comprehension_item(kind, expected, actual, value, false);
            }
            // The contextual item error describes this fresh construction.
            // Keep normal list/set recovery and avoid a second argument error.
            Some(expected)
        });
        self.scopes = previous_scopes;
        self.flow = previous_flow;
        self.try_inputs = previous_inputs;
        self.checked_expression_owner = previous_owner;
        self.checked_expression_types = previous_types;
        self.checked_expression_contexts = previous_contexts;
        self.expected_value = previous_expected;
        Some(actual)
    }

    fn check_dictionary_comprehension_item(
        &mut self,
        kind: &str,
        expected: Type,
        actual: Type,
        value: &Expr,
        provisional: bool,
    ) -> bool {
        if self.context.accepts(expected, actual) {
            return true;
        }
        if !provisional {
            self.emit(
                "misc",
                format!(
                    "{kind} expression in dictionary comprehension has incompatible type \"{}\"; expected type \"{}\"",
                    self.context.name(actual), self.context.name(expected),
                ),
                value,
            );
        }
        false
    }

    pub(in super::super) fn check_candidate_dictionary(
        &mut self,
        dictionary: &quality_syntax::python_ast::ExprDictComp,
        expected: Option<Type>,
    ) -> super::super::generator_inputs::CheckedExpression {
        use super::super::generator_inputs::{unknown_expression, CandidateFailure, CheckedValue};
        let Some(key) = dictionary.key.as_deref() else {
            return unknown_expression(dictionary, "unpacked dictionary comprehension");
        };
        if dictionary.generators.is_empty()
            || dictionary
                .generators
                .iter()
                .any(|clause| clause.is_async || !clause.ifs.is_empty())
        {
            return unknown_expression(dictionary, "async or filtered dictionary comprehension");
        }
        let contexts = match expected {
            None => [None, None],
            Some(expected) => {
                let expected = self.dictionary_comprehension_context(expected);
                let Some(node) = self.context.node(expected) else {
                    return unknown_expression(
                        dictionary,
                        "unknown dictionary comprehension context",
                    );
                };
                match node.as_ref() {
                    Node::Container(Container::Dict, arguments) => {
                        let [key, value] = arguments.as_slice() else {
                            return unknown_expression(
                                dictionary,
                                "invalid dictionary comprehension context arity",
                            );
                        };
                        [Some(*key), Some(*value)]
                    }
                    Node::Container(Container::Iterable, arguments) => {
                        let [key] = arguments.as_slice() else {
                            return unknown_expression(
                                dictionary,
                                "invalid dictionary iterable context arity",
                            );
                        };
                        // A dictionary iterates over keys. No value context is
                        // invented for this actual represented Iterable input.
                        [Some(*key), None]
                    }
                    _ => {
                        return unknown_expression(
                            dictionary,
                            "unproved dictionary comprehension context",
                        )
                    }
                }
            }
        };
        let provisional = self.provisional_generator_context;
        self.check_candidate_comprehension(&dictionary.generators, |checker, mut children| {
            let key_actual = checker.check_candidate_expression(key, contexts[0])?;
            let value_actual =
                checker.check_candidate_expression(&dictionary.value, contexts[1])?;
            let key_type = key_actual.actual();
            let value_type = value_actual.actual();
            children.extend([key_actual, value_actual]);
            let mut compatible = true;
            for (kind, expected, actual, value) in [
                ("Key", contexts[0], key_type, key),
                ("Value", contexts[1], value_type, dictionary.value.as_ref()),
            ] {
                if let Some(expected) = expected {
                    compatible &= checker.check_dictionary_comprehension_item(
                        kind,
                        expected,
                        actual,
                        value,
                        provisional,
                    );
                }
            }
            if !compatible {
                if provisional {
                    return unknown_expression(
                        dictionary,
                        "rejected provisional dictionary context",
                    );
                }
                if !children.iter().any(CheckedValue::is_recovered) {
                    return Err(CandidateFailure::Rejected);
                }
            }
            // A clean fresh construction uses its proved item contexts, as
            // normal contextual dictionary inference does. Error recovery keeps
            // the actual children and origins instead of guessing an expected type.
            let contextual = compatible && !children.iter().any(CheckedValue::is_recovered);
            let key = if contextual {
                contexts[0].unwrap_or(key_type)
            } else {
                key_type
            };
            let value = if contextual {
                contexts[1].unwrap_or(value_type)
            } else {
                value_type
            };
            let actual = checker
                .context
                .intern(Node::Container(Container::Dict, vec![key, value]));
            Ok(CheckedValue::combine(actual, children))
        })
    }

    fn infer_list_comprehension_expected(
        &mut self,
        expression: &Expr,
        expected: Type,
    ) -> Option<Type> {
        let Expr::ListComp(list) = expression else {
            return None;
        };
        let expected_node = self.context.node(expected)?;
        let Node::Container(Container::List, expected_items) = expected_node.as_ref() else {
            return None;
        };
        let expected_item = *expected_items.first()?;
        // Scopes owns the filter/lexical resolver. Do not rebuild its environment here.
        let actual = self.infer(expression)?;
        let actual_node = self.context.node(actual)?;
        let Node::Container(Container::List, actual_items) = actual_node.as_ref() else {
            return None;
        };
        let actual_item = *actual_items.first()?;
        if !self.context.accepts(expected_item, actual_item) {
            self.emit(
                "misc",
                format!(
                    "List comprehension has incompatible type List[{}]; expected List[{}]",
                    self.context.name(actual_item),
                    self.context.name(expected_item)
                ),
                list.elt.as_ref(),
            );
        }
        // Match the existing contextual container rule; do not add return-value.
        Some(expected)
    }

    fn infer_set_comprehension_expected(
        &mut self,
        expression: &Expr,
        expected: Type,
    ) -> Option<Type> {
        let Expr::SetComp(set) = expression else {
            return None;
        };
        if !types::supported_set_comprehension_generators(&set.generators) {
            return None;
        }
        let expected_node = self.context.node(expected)?;
        let Node::Container(Container::Set, expected_items) = expected_node.as_ref() else {
            return None;
        };
        let expected_item = *expected_items.first()?;
        let actual = self.infer(expression)?;
        let actual_node = self.context.node(actual)?;
        let Node::Container(Container::Set, actual_items) = actual_node.as_ref() else {
            return None;
        };
        let actual_item = *actual_items.first()?;
        if !self.context.accepts(expected_item, actual_item) {
            self.emit(
                "misc",
                format!(
                    "Set comprehension has incompatible type Set[{}]; expected Set[{}]",
                    self.context.name(actual_item),
                    self.context.name(expected_item)
                ),
                set.elt.as_ref(),
            );
            // The element error already describes this contextual construction.
            return Some(expected);
        }
        Some(expected)
    }
}

#[cfg(test)]
mod candidate_dictionary_tests {
    use crate::domain::{
        model::{QualityLocation, QualityTool},
        ports::ProjectStore,
        source::PythonVersion,
    };
    use crate::infrastructure::filesystem::Filesystem;
    use crate::infrastructure::mypy::{
        checker::{
            generator_inputs::{CandidateFailure, CheckedValue},
            Checker,
        },
        dependencies, modules, roots,
        types::{self, Container, Node, Type},
    };
    use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt};
    use std::{
        collections::BTreeSet,
        rc::Rc,
        sync::{Arc, Mutex},
    };

    #[test]
    fn dictionary_update_checks_contextual_value_and_keeps_valid_value_clean() {
        let source = r#"priority: dict[str, int] = {"app:workflow": 1}
operations = ["batch"]
priority.update({symbol: priority["app:workflow"] - 0.5 + index / 100
                 for index, symbol in enumerate(operations)})
priority.update({symbol: index for index, symbol in enumerate(operations)})
priority.update({index: index for index, symbol in enumerate(operations)})
"#;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.py");
        std::fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;

        let report = crate::infrastructure::mypy::check(&workspace);
        let diagnostics: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                assert_eq!(diagnostic.tool, QualityTool::Mypy);
                assert_eq!(diagnostic.path, path);
                assert!(diagnostic.fix.is_none());
                (
                    diagnostic.rule.as_str(),
                    diagnostic.message.as_str(),
                    diagnostic.hint.as_deref(),
                    diagnostic.severity.as_str(),
                    diagnostic.location.clone(),
                )
            })
            .collect();
        assert_eq!(
            diagnostics,
            [
                (
                    "misc",
                    "Value expression in dictionary comprehension has incompatible type \"float\"; expected type \"int\"",
                    None,
                    "error",
                    QualityLocation {
                        line: 3,
                        column: 26,
                        end_line: Some(3),
                        end_column: Some(69),
                        offset: Some(95),
                        end_offset: Some(139),
                    },
                ),
                (
                    "misc",
                    "Key expression in dictionary comprehension has incompatible type \"int\"; expected type \"str\"",
                    None,
                    "error",
                    QualityLocation {
                        line: 6,
                        column: 18,
                        end_line: Some(6),
                        end_column: Some(22),
                        offset: Some(295),
                        end_offset: Some(300),
                    },
                ),
            ]
        );
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    }

    #[test]
    fn iterable_overload_context_checks_dictionary_keys_without_value_context() {
        let source = r#"from typing import Iterable, overload

@overload
def consume(values: Iterable[str]) -> int: ...
@overload
def consume(values: Iterable[int]) -> str: ...
def consume(values: object) -> int | str:
    return 1

reveal_type(consume({key: 0.5 for key in ("one",)}))
reveal_type(consume({key: 0.5 for key in (1,)}))
"#;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.py");
        std::fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;

        let report = crate::infrastructure::mypy::check(&workspace);
        let diagnostics: Vec<_> = report
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.rule.as_str(),
                    diagnostic.message.as_str(),
                    diagnostic.severity.as_str(),
                )
            })
            .collect();
        assert_eq!(
            diagnostics,
            [
                ("misc", "Revealed type is \"int\"", "note"),
                ("misc", "Revealed type is \"str\"", "note"),
            ]
        );
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    }

    #[test]
    fn candidate_dictionary_proves_key_only_context_and_rejects_wrong_key() {
        let source = "{key: 0.5 for key in (\"one\",)}\n{key: 0.5 for key in (1,)}\n";
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("main.py");
        std::fs::write(&path, source).unwrap();
        let mut workspace = Filesystem.load(std::slice::from_ref(&path), true);
        workspace.projects[0].python_version = PythonVersion::Py312;

        let checked_source_count = workspace.sources.len();
        let prepared = roots::prepare(&workspace).expect("source roots");
        let loaded = dependencies::load(prepared.workspace, prepared.names);
        let context = Rc::new(types::Context::default());
        let index = loaded.index();
        let installed_sources: BTreeSet<_> = (checked_source_count..loaded.workspace.sources.len())
            .filter(|&id| {
                !loaded.local.contains(&id)
                    && !loaded.workspace.sources[id].is_stub
                    && !crate::infrastructure::mypy::dependencies::embedded::is_source(
                        &loaded.workspace.sources[id],
                    )
            })
            .collect();
        let modules = modules::Modules::build(
            &index,
            context.clone(),
            &loaded.workspace,
            loaded.untyped.clone(),
            loaded.imported.clone(),
            installed_sources,
        );
        let document = index.modules[0].document.as_ref().expect("parsed source");
        let [Stmt::Expr(first), Stmt::Expr(second)] = document.parsed.syntax().body.as_slice()
        else {
            panic!("two dictionary comprehension expressions");
        };
        let Expr::DictComp(first) = first.value.as_ref() else {
            panic!("first dictionary comprehension");
        };
        let Expr::DictComp(second) = second.value.as_ref() else {
            panic!("second dictionary comprehension");
        };
        let mut import_notes = BTreeSet::new();
        let mut checker = Checker::new(
            &loaded.workspace.sources[0],
            document,
            context.clone(),
            modules::Modules::lookup(&modules, 0),
            &mut import_notes,
        );

        for (dictionary, key) in [(first, Type::Str), (second, Type::Int)] {
            let expected = context.intern(Node::Container(Container::Iterable, vec![key]));
            let wrong = context.intern(Node::Container(
                Container::Iterable,
                vec![if key == Type::Str {
                    Type::Int
                } else {
                    Type::Str
                }],
            ));
            let actual = context.intern(Node::Container(Container::Dict, vec![key, Type::Float]));
            for (context_type, accepted) in [(expected, true), (wrong, false)] {
                let local_types = Arc::new(Mutex::new(Default::default()));
                checker.contextual_candidate_depth = 1;
                checker.typed_dict_trial_failed = Some(false);
                checker.checked_expression_types = Some(local_types.clone());
                checker.checked_expression_owner = Some(checker.scopes.try_input_scope());
                let result = context.with_checked_expression_types(&local_types, || {
                    checker.check_candidate_dictionary(dictionary, Some(context_type))
                });
                if accepted {
                    assert!(matches!(result, Ok(CheckedValue::Clean(ty)) if ty == actual));
                    assert_eq!(checker.typed_dict_trial_failed, Some(false));
                } else {
                    assert!(matches!(result, Err(CandidateFailure::Rejected)));
                    assert_eq!(checker.typed_dict_trial_failed, Some(true));
                }
                checker.contextual_candidate_depth = 0;
                checker.typed_dict_trial_failed = None;
                checker.checked_expression_types = None;
                checker.checked_expression_owner = None;
            }
        }
        assert_eq!(std::fs::read(&path).unwrap(), source.as_bytes());
    }
}
