//! Match diagnostics preserve raw patterns and statically resolved class metadata.
use super::{callables::Target, classes::Class, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, Pattern, PatternMatchClass, StmtAssign, StmtMatch,
};
use ruff_text_size::Ranged;
use std::{collections::BTreeSet, rc::Rc};

fn matches_self(class: &Class) -> bool {
    class.identity.0 == "builtins"
        && matches!(
            class.name.as_str(),
            "bool"
                | "bytearray"
                | "bytes"
                | "dict"
                | "float"
                | "frozenset"
                | "int"
                | "list"
                | "set"
                | "str"
                | "tuple"
        )
}

fn string_tuple(value: &Expr) -> Option<Vec<String>> {
    let Expr::Tuple(value) = value else {
        return None;
    };
    value
        .elts
        .iter()
        .map(|value| {
            if let Expr::StringLiteral(value) = value {
                Some(value.value.to_str().to_owned())
            } else {
                None
            }
        })
        .collect()
}

impl Checker<'_> {
    pub(super) fn check_match_definition(&mut self, assignment: &StmtAssign) {
        if !self.scopes.in_class() || string_tuple(&assignment.value).is_some() {
            return;
        }
        for target in &assignment.targets {
            if matches!(target, Expr::Name(value) if value.id.as_str() == "__match_args__") {
                self.emit(
                    "E1902",
                    "invalid-match-args-definition",
                    "`__match_args__` must be a tuple of strings.",
                    assignment.value.range(),
                );
            }
        }
    }

    pub(super) fn check_match_captures(&mut self, statement: &StmtMatch) {
        for case in statement
            .cases
            .iter()
            .take(statement.cases.len().saturating_sub(1))
        {
            if case.guard.is_some() {
                continue;
            }
            if let Pattern::MatchAs(pattern) = &case.pattern {
                if pattern.pattern.is_none() {
                    if let Some(name) = &pattern.name {
                        self.emit(
                            "E1901", "bare-name-capture-pattern",
                            &format!("The name capture `case {name}` makes the remaining patterns unreachable. Use a dotted name (for example an enum) to fix this."),
                            pattern.range(),
                        );
                    }
                }
            }
        }
    }

    fn match_arguments(&self, class: Rc<Class>, context: Context) -> Option<Vec<String>> {
        for (owner, _) in self.lineage(class.clone(), context) {
            if let Some(value) = owner.expressions.get("__match_args__") {
                return string_tuple(value);
            }
            if owner.declared_members.contains("__match_args__") {
                return None;
            }
        }
        matches_self(&class).then(|| vec!["<self>".into()])
    }

    fn duplicate_match_attribute(
        &mut self,
        name: &str,
        node: &PatternMatchClass,
        attributes: &mut BTreeSet<String>,
        duplicates: &mut BTreeSet<String>,
    ) {
        if attributes.contains(name) && duplicates.insert(name.to_owned()) {
            self.emit(
                "E1904",
                "multiple-class-sub-patterns",
                &format!("Multiple sub-patterns for attribute {name}"),
                node.range(),
            );
        } else {
            attributes.insert(name.to_owned());
        }
    }

    pub(super) fn check_match_class(&mut self, node: &PatternMatchClass) {
        let inferred = self.expression_target(&node.cls).and_then(|resolved| {
            if let Target::Class(class) = resolved.target {
                Some((class, resolved.context))
            } else {
                None
            }
        });
        let mut attributes = BTreeSet::new();
        let mut duplicates = BTreeSet::new();
        if let Some((class, context)) = &inferred {
            if !node.arguments.patterns.is_empty() {
                if let Some(names) = self.match_arguments(class.clone(), context.clone()) {
                    if node.arguments.patterns.len() > names.len() {
                        self.emit(
                            "E1903",
                            "too-many-positional-sub-patterns",
                            &format!(
                                "{} expects {} positional sub-patterns (given {})",
                                super::display::expression(&node.cls, &self.source.text),
                                names.len(),
                                node.arguments.patterns.len()
                            ),
                            node.range(),
                        );
                        return;
                    }
                    if !matches_self(class)
                        && !class.bases.iter().any(
                            |base| matches!(base, Expr::Name(name) if name.id.as_str() == "tuple"),
                        )
                    {
                        let names = names
                            .iter()
                            .take(node.arguments.patterns.len())
                            .map(|name| format!("'{name}'"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        self.emit(
                            "R1906",
                            "match-class-positional-attributes",
                            &format!("Use keyword attributes instead of positional ones ({names})"),
                            node.range(),
                        );
                    }
                    for name in names.iter().take(node.arguments.patterns.len()) {
                        self.duplicate_match_attribute(
                            name,
                            node,
                            &mut attributes,
                            &mut duplicates,
                        );
                    }
                }
            }
        }
        for keyword in &node.arguments.keywords {
            self.duplicate_match_attribute(
                keyword.attr.as_str(),
                node,
                &mut attributes,
                &mut duplicates,
            );
        }
        if let (Expr::Name(name), Some((class, _)), [Pattern::MatchAs(pattern)]) = (
            node.cls.as_ref(),
            inferred,
            node.arguments.patterns.as_slice(),
        ) {
            if matches_self(&class) && pattern.pattern.is_none() {
                if let Some(binding) = &pattern.name {
                    self.emit(
                        "R1905",
                        "match-class-bind-self",
                        &format!("Use '{}() as {binding}' instead", name.id),
                        pattern.range(),
                    );
                }
            }
        }
    }
}
