//! Config values preserve the difference between false and unspecified.
use super::{text, Model};
use mamba_language::application::quality_syntax::python_ast::{Expr, Stmt, StmtClassDef};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub(in crate::infrastructure::mypy) struct Values(pub BTreeMap<String, bool>);

impl Values {
    pub(super) fn inherit(&mut self, parent: &Self) {
        for (key, value) in &parent.0 {
            self.0.entry(key.clone()).or_insert(*value);
        }
    }

    fn update(&mut self, key: &str, value: &Expr) {
        let result = match key {
            "alias_generator" => Some(!matches!(value, Expr::NoneLiteral(_))),
            "extra" => match value {
                Expr::StringLiteral(string) => Some(string.value.to_str() == "forbid"),
                Expr::Attribute(member) => Some(member.attr.as_str() == "forbid"),
                _ => None,
            },
            "frozen" | "from_attributes" | "populate_by_name" | "validate_by_alias"
            | "validate_by_name" | "strict" => match value {
                Expr::BooleanLiteral(boolean) => Some(boolean.value),
                _ => None,
            },
            _ => None,
        };
        if let Some(result) = result {
            self.0.insert(key.into(), result);
        }
    }

    pub(super) fn apply(&self, model: &mut Model) {
        let flag = |key: &str| self.0.get(key).copied().unwrap_or(false);
        model.frozen = flag("frozen");
        model.populate_by_name = flag("populate_by_name");
        model.validate_by_name = flag("validate_by_name");
        model.validate_by_alias = self.0.get("validate_by_alias").copied();
        model.alias_generator = flag("alias_generator");
        model.from_attributes = self.0.get("from_attributes").copied();
    }
}

pub(super) fn local(class: &StmtClassDef) -> Values {
    let mut values = Values::default();
    visit(class, |key, value, _| values.update(key, value));
    values
}

pub(in crate::infrastructure::mypy) fn tracked(key: &str) -> bool {
    matches!(
        key,
        "extra"
            | "frozen"
            | "from_attributes"
            | "populate_by_name"
            | "validate_by_alias"
            | "validate_by_name"
            | "alias_generator"
            | "strict"
    )
}

pub(in crate::infrastructure::mypy) fn valid(key: &str, value: &Expr, lax_extra: bool) -> bool {
    match key {
        "alias_generator" => true,
        "extra" => lax_extra || matches!(value, Expr::StringLiteral(_) | Expr::Attribute(_)),
        _ => !tracked(key) || matches!(value, Expr::BooleanLiteral(_)),
    }
}

pub(in crate::infrastructure::mypy) fn visit<'a>(
    class: &'a StmtClassDef,
    mut apply: impl FnMut(&str, &'a Expr, bool),
) {
    if let Some(arguments) = &class.arguments {
        for keyword in &arguments.keywords {
            if let Some(key) = &keyword.arg {
                apply(key.as_str(), &keyword.value, false);
            }
        }
    }
    for statement in &class.body {
        match statement {
            Stmt::Assign(assignment) if matches!(assignment.targets.first(), Some(Expr::Name(name)) if name.id.as_str() == "model_config") =>
            {
                model_config(&assignment.value, &mut apply);
            }
            Stmt::AnnAssign(assignment) if matches!(assignment.target.as_ref(), Expr::Name(name) if name.id.as_str() == "model_config") => {
                if let Some(value) = assignment.value.as_deref() {
                    model_config(value, &mut apply);
                }
            }
            Stmt::ClassDef(config) if config.name.as_str() == "Config" => {
                for statement in &config.body {
                    match statement {
                        Stmt::Assign(assignment) => {
                            if let Some(Expr::Name(key)) = assignment.targets.first() {
                                apply(key.id.as_str(), &assignment.value, false);
                            }
                        }
                        Stmt::AnnAssign(assignment) => {
                            if let (Expr::Name(key), Some(value)) =
                                (assignment.target.as_ref(), assignment.value.as_deref())
                            {
                                apply(key.id.as_str(), value, false);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn model_config<'a>(value: &'a Expr, apply: &mut impl FnMut(&str, &'a Expr, bool)) {
    match value {
        Expr::Call(call) => {
            for keyword in &call.arguments.keywords {
                if let Some(key) = &keyword.arg {
                    apply(key.as_str(), &keyword.value, true);
                }
            }
        }
        Expr::Dict(dictionary) => {
            for item in &dictionary.items {
                if let Some(key) = item.key.as_ref().and_then(text) {
                    apply(&key, &item.value, false);
                }
            }
        }
        _ => {}
    }
}
