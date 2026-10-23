//! Module exports inspect the original first inferred definition and each source item.
use super::{callables::Target, display, operand_inference::Operand, resolution::Context, Checker};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, FStringPartRef, InterpolatedStringElement,
};
use ruff_text_size::{Ranged, TextRange};
use std::collections::BTreeSet;

fn fstring_constant_range(value: &Expr) -> Option<TextRange> {
    let Expr::FString(value) = value else {
        return None;
    };
    let mut ranges = Vec::new();
    for part in value.value.iter() {
        match part {
            FStringPartRef::Literal(value) => ranges.push(value.range()),
            FStringPartRef::FString(value) => {
                for element in &value.elements {
                    match element {
                        InterpolatedStringElement::Literal(value) => ranges.push(value.range()),
                        InterpolatedStringElement::Interpolation(_) => return None,
                    }
                }
            }
        }
    }
    (ranges.len() == 1).then(|| ranges[0])
}

impl Checker<'_> {
    fn export_source(&self, origin: &Context) -> &str {
        match origin {
            Context::Module(module) => self
                .modules
                .source(self.source.project, module)
                .unwrap_or(&self.source.text),
            Context::Scope(_) => &self.source.text,
        }
    }
    pub(super) fn check_exports(&mut self) {
        let Some(expression) = self
            .scopes
            .first_expression("__all__", self.scopes.current_index())
            .cloned()
        else {
            return;
        };
        let origin = Context::Scope(self.scopes.current_index());
        let Some((expression, origin)) = self.operand_nodes(&expression, origin).into_iter().next()
        else {
            return;
        };
        if matches!(expression, Expr::Generator(_)) {
            return;
        }
        let collection = self.unpacked_value(&expression, origin.clone());
        let valid = collection
            .as_ref()
            .is_some_and(|value| matches!(value.kind, "list" | "tuple"));
        if !valid {
            let constant_range = fstring_constant_range(&expression);
            let known = matches!(expression, Expr::EllipsisLiteral(_) | Expr::Lambda(_))
                || collection.is_some()
                || constant_range.is_some()
                || !matches!(self.operand(&expression), Operand::Unknown);
            if known {
                let omit_end = if let Expr::Call(call) = &expression {
                    !call.arguments.args.is_empty()
                        && self.expression_target_in(&call.func, origin.clone()).is_some_and(|value|matches!(value.target, Target::Class(class) if class.identity.0 == "builtins" && class.name == "set"))
                } else {
                    false
                };
                let before = self.diagnostics.len();
                self.emit_operand_origin(
                    &origin,
                    "E0605",
                    "invalid-all-format",
                    "Invalid format for __all__, must be tuple or list",
                    constant_range.unwrap_or(expression.range()),
                );
                if omit_end && self.diagnostics.len() > before {
                    self.diagnostics[before].location.end_line = None;
                    self.diagnostics[before].location.end_column = None;
                }
            }
            return;
        }
        let collection = collection.expect("valid export collection");
        for item in collection.elements.unwrap_or_default() {
            let Some((inferred, _)) = self
                .operand_nodes(&item, collection.origin.clone())
                .into_iter()
                .next()
            else {
                continue;
            };
            let Expr::StringLiteral(string) = &inferred else {
                if matches!(inferred, Expr::Name(_))
                    && matches!(self.operand(&inferred), Operand::Unknown)
                {
                    continue;
                }
                let spelling = display::expression(&item, self.export_source(&collection.origin));
                let spelling = super::string_format::python_string_repr(&spelling);
                self.emit_operand_origin(
                    &collection.origin,
                    "E0604",
                    "invalid-all-object",
                    &format!("Invalid object {spelling} in __all__, must contain only strings"),
                    item.range(),
                );
                continue;
            };
            let name = string.value.to_str();
            let missing = self.scopes.exports(&BTreeSet::from([name.to_string()]));
            if missing.contains(name)
                && !(self.is_package
                    && self
                        .modules
                        .exists(self.source.project, &format!("{}.{name}", self.module_name)))
            {
                self.emit_operand_origin(
                    &collection.origin,
                    "E0603",
                    "undefined-all-variable",
                    &format!("Undefined variable name '{name}' in __all__"),
                    item.range(),
                );
            }
        }
    }
}
