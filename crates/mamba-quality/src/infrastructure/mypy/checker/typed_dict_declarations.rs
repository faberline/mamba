//! Typed dictionary declarations validate syntax and inherited field constraints.
use super::super::classes::typed_dict::Definition;
use super::*;
use quality_syntax::python_ast::{Arguments, ExprDict, StmtAssign, StmtClassDef};
use std::collections::BTreeSet;
use types::{Node, Type};

impl Checker<'_> {
    pub(super) fn infer_typed_dict_union(
        &mut self,
        dictionary: &ExprDict,
        expected: Type,
    ) -> Option<Type> {
        if !matches!(self.context.node(expected).as_deref(), Some(Node::Union(_))) {
            return None;
        }
        let (candidates, exhaustive) =
            self.typed_dict_contexts(dictionary, expected, &mut BTreeSet::new());
        if candidates.len() == 1 && exhaustive {
            return self.check_typed_dict_literal(dictionary, candidates[0], dictionary);
        }
        for &candidate in &candidates {
            let calls = self.expected_calls.clone();
            let outer_trial = self.typed_dict_trial_failed.replace(false);
            let context = self.context.clone();
            let actual = context.trial_alias_origins(|| {
                let actual = self.check_typed_dict_literal(dictionary, candidate, dictionary);
                let clean = actual.is_some() && self.typed_dict_trial_failed == Some(false);
                (actual.filter(|_| clean), clean)
            });
            self.typed_dict_trial_failed = outer_trial;
            if actual.is_some() {
                return actual;
            }
            self.expected_calls = calls;
        }
        if exhaustive && !candidates.is_empty() {
            let names = candidates
                .iter()
                .map(|&ty| format!("\"{}\"", self.context.name(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            self.emit(
                "misc",
                format!("Type of TypedDict is ambiguous, none of ({names}) matches cleanly"),
                dictionary,
            );
        }
        None
    }

    fn typed_dict_contexts(
        &self,
        dictionary: &ExprDict,
        expected: Type,
        active: &mut BTreeSet<Type>,
    ) -> (Vec<Type>, bool) {
        if !active.insert(expected) {
            return (Vec::new(), false);
        }
        let expanded = self.context.expanded_alias_instance(expected);
        let result = if let Some((class, _)) = self.context.typed_dict(expanded) {
            if class.generic.is_empty() {
                (vec![expanded], true)
            } else {
                (Vec::new(), false)
            }
        } else if let Some(Node::Union(items)) = self.context.node(expanded).as_deref() {
            let mut candidates = Vec::new();
            let mut exhaustive = true;
            for &item in items {
                let (found, complete) = self.typed_dict_contexts(dictionary, item, active);
                exhaustive &= complete;
                for candidate in found {
                    let context = self.context.clone();
                    let matches = context.trial_alias_origins(|| {
                        (self.typed_dict_literal_shape(dictionary, candidate), false)
                    });
                    if matches {
                        candidates.push(candidate);
                    }
                }
            }
            (candidates, exhaustive)
        } else {
            (Vec::new(), false)
        };
        active.remove(&expected);
        result
    }

    fn typed_dict_literal_shape(&self, dictionary: &ExprDict, expected: Type) -> bool {
        let Some((class, definition)) = self.context.typed_dict(expected) else {
            return false;
        };
        if !class.generic.is_empty() && dictionary.items.iter().any(|item| item.key.is_none()) {
            return false;
        }
        let mut keys = BTreeSet::new();
        for item in &dictionary.items {
            if let Some(key) = &item.key {
                let Some(mut names) = self.context.typed_dict_keys(key, &self.environment()) else {
                    return false;
                };
                names.sort();
                names.dedup();
                if names.len() != 1 {
                    return false;
                }
                keys.extend(names);
            } else {
                let Some(actual) = self.infer(&item.value) else {
                    return false;
                };
                let actual = self.context.expanded_alias_instance(actual);
                let Some((class, fields)) = self.context.typed_dict(actual) else {
                    return false;
                };
                // Match the existing unpack validator's supported input boundary.
                if self.context.is_class_value(actual) || !class.generic.is_empty() {
                    return false;
                }
                keys.extend(fields.items.into_iter().map(|field| field.name));
            }
        }
        definition
            .items
            .iter()
            .all(|field| !field.required || keys.contains(&field.name))
            && keys
                .iter()
                .all(|key| definition.items.iter().any(|field| &field.name == key))
    }

    fn check_typed_dict_total(&mut self, arguments: &Arguments) {
        for keyword in &arguments.keywords {
            if keyword
                .arg
                .as_ref()
                .is_some_and(|name| name.as_str() == "total")
                && !matches!(&keyword.value, Expr::BooleanLiteral(_))
            {
                self.emit(
                    "literal-required",
                    "\"total\" argument must be a True or False literal".into(),
                    &keyword.value,
                );
            }
        }
    }

    pub(super) fn check_typed_dict_declaration(&mut self, class: &StmtClassDef) {
        let parents: Vec<_> = class
            .arguments
            .iter()
            .flat_map(|arguments| &arguments.args)
            .filter_map(|base| {
                self.scopes
                    .annotation(base)
                    .and_then(|ty| self.context.typed_dict(ty))
            })
            .collect();
        if let Some(arguments) = &class.arguments {
            self.check_typed_dict_total(arguments);
        }
        let ty = self.scopes.class_object(class.name.as_str()).unwrap();
        let (_, definition) = self.context.typed_dict(ty).unwrap();
        let mut fields = BTreeSet::new();
        for child in &class.body {
            match child {
                Stmt::AnnAssign(assignment) => {
                    self.check_callable_annotation(&assignment.annotation);
                    let Expr::Name(name) = assignment.target.as_ref() else {
                        self.invalid_typed_dict_statement(child);
                        continue;
                    };
                    if !fields.insert(name.id.to_string()) {
                        self.emit(
                            "misc",
                            format!("Duplicate TypedDict key \"{}\"", name.id),
                            child,
                        );
                    }
                    if assignment.value.is_some() {
                        self.emit(
                            "misc",
                            "Right hand side values are not supported in TypedDict".into(),
                            child,
                        );
                    }
                    let Some(item) = definition
                        .items
                        .iter()
                        .find(|item| item.name == name.id.as_str())
                    else {
                        continue;
                    };
                    for (parent, inherited) in &parents {
                        if let Some(previous) = inherited
                            .items
                            .iter()
                            .find(|previous| previous.name == item.name)
                        {
                            if !self.context.typed_dict_accepts(
                                &Definition {
                                    closed: false,
                                    items: vec![previous.clone()],
                                },
                                &Definition {
                                    closed: false,
                                    items: vec![item.clone()],
                                },
                            ) {
                                self.emit("misc", format!("Definition of field \"{}\" incompatible with base class \"{}\"", item.name, parent.name), child);
                            }
                        }
                    }
                }
                Stmt::Pass(_) => {}
                Stmt::Expr(expression)
                    if matches!(
                        expression.value.as_ref(),
                        Expr::StringLiteral(_) | Expr::EllipsisLiteral(_)
                    ) => {}
                _ => self.invalid_typed_dict_statement(child),
            }
        }
    }

    fn invalid_typed_dict_statement(&mut self, child: &Stmt) {
        self.emit(
            "misc",
            "Invalid statement in TypedDict definition; expected \"field_name: field_type\"".into(),
            child,
        );
    }

    pub(super) fn check_functional_typed_dict(&mut self, assignment: &StmtAssign) {
        let ([Expr::Name(name)], Expr::Call(call)) =
            (assignment.targets.as_slice(), assignment.value.as_ref())
        else {
            return;
        };
        self.check_typed_dict_total(&call.arguments);
        if let Some(Expr::StringLiteral(literal)) = call.arguments.args.first() {
            if literal.value.to_str() != name.id.as_str() {
                self.emit(
                    "name-match",
                    format!(
                        "First argument \"{}\" to TypedDict() does not match variable name \"{}\"",
                        literal.value.to_str(),
                        name.id
                    ),
                    call,
                );
            }
        }
    }
}
