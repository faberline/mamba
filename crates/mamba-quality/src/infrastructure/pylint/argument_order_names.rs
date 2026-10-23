//! Named call-site arguments retain source nodes and keyword replacement order.
use super::{resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use std::collections::BTreeMap;

pub(super) struct Names {
    pub positional: Vec<String>,
    pub keywords: Vec<String>,
}
fn name(expression: &Expr) -> Option<String> {
    let Expr::Name(value) = expression else {
        return None;
    };
    Some(value.id.to_string())
}
pub(super) fn collect(checker: &Checker<'_>, call: &ExprCall) -> Option<Names> {
    let origin = Context::Scope(checker.scopes.current_index());
    let mut positional = Vec::new();
    for argument in &call.arguments.args {
        if let Expr::Starred(value) = argument {
            let values = checker.unpacked_value(&value.value, origin.clone())?;
            if !matches!(values.kind, "tuple" | "list" | "set") {
                return None;
            }
            for value in values.elements? {
                positional.push(name(&value)?);
            }
        } else {
            positional.push(name(argument)?);
        }
    }
    let mut keywords = BTreeMap::<String, Option<String>>::new();
    for keyword in &call.arguments.keywords {
        if let Some(key) = &keyword.arg {
            keywords.insert(key.to_string(), name(&keyword.value));
            continue;
        }
        let values = checker.unpacked_value(&keyword.value, origin.clone())?;
        if !values.dictionary || values.view.is_some() {
            return None;
        }
        for (key, value) in values.entries? {
            let Expr::StringLiteral(key) = checker.stdlib_constant(&key, values.origin.clone())?
            else {
                return None;
            };
            let key = key.value.to_str().to_string();
            let value = if keywords.contains_key(&key) {
                None
            } else {
                name(&value)
            };
            keywords.insert(key, value);
        }
    }
    Some(Names {
        positional,
        keywords: keywords.into_values().collect::<Option<Vec<_>>>()?,
    })
}
