//! Unpacked call values retain uncertainty, duplicates, and source-key order.
use super::{values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct Arguments {
    pub positional: usize,
    pub keywords: Vec<String>,
    pub duplicates: Vec<String>,
    pub invalid: bool,
    pub might_supply_keywords: bool,
}

impl Arguments {
    pub(super) fn from_call(checker: &Checker<'_>, call: &ExprCall) -> Self {
        let mut arguments = Self::default();
        for expression in &call.arguments.args {
            if let Expr::Starred(starred) = expression {
                match checker.static_value(&starred.value) {
                    Value::Sequence(items, _) => arguments.positional += items.len(),
                    _ => arguments.invalid = true,
                }
            } else {
                arguments.positional += 1;
            }
        }
        let mut keys = BTreeSet::new();
        for keyword in &call.arguments.keywords {
            if let Some(name) = &keyword.arg {
                if keys.insert(name.to_string()) {
                    arguments.keywords.push(name.to_string());
                }
            } else {
                arguments.might_supply_keywords |= !matches!(keyword.value, Expr::Dict(_));
                match checker.static_value(&keyword.value) {
                    Value::Mapping(items) => {
                        for (key, _) in items {
                            if let Some(Value::String(name)) = key {
                                if keys.insert(name.clone()) {
                                    arguments.keywords.push(name);
                                } else {
                                    arguments.invalid = true;
                                    if !arguments.duplicates.contains(&name) {
                                        arguments.duplicates.push(name);
                                    }
                                }
                            } else {
                                arguments.invalid = true;
                            }
                        }
                    }
                    _ => arguments.invalid = true,
                }
            }
        }
        arguments
    }
}
