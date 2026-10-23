//! Percent diagnostics retain syntax-node exemptions and inferred argument types.
use super::super::logging::percent::{parse, Error, Format};
use super::super::resolution::Resolved;
use super::*;
use mamba_language::application::quality_syntax::python_ast::{ExprBinOp, ExprDict};
use std::rc::Rc;

fn node_name(expression: &Expr) -> Option<&'static str> {
    match expression {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_) => Some("Const"),
        Expr::List(_) => Some("List"),
        Expr::Lambda(_) => Some("Lambda"),
        Expr::ListComp(_) => Some("ListComp"),
        Expr::SetComp(_) => Some("SetComp"),
        Expr::Generator(_) => Some("GeneratorExp"),
        _ => None,
    }
}

impl Checker<'_> {
    pub(super) fn check_percent_string(&mut self, binary: &ExprBinOp) {
        let Expr::StringLiteral(text) = binary.left.as_ref() else {
            return;
        };
        let format = match parse(text.value.to_str()) {
            Ok(format) => format,
            Err(error) => {
                match error {
                    Error::Truncated => self.emit(
                        "E1301",
                        "truncated-format-string",
                        "Format string ends in middle of conversion specifier",
                        binary.range(),
                    ),
                    Error::Unsupported(c, index) => self.emit(
                        "E1300",
                        "bad-format-character",
                        &format!(
                            "Unsupported format character {} ({:#x}) at index {index}",
                            super::super::logging::percent::repr(c),
                            u32::from(c)
                        ),
                        binary.range(),
                    ),
                }
                return;
            }
        };
        if !format.mapping && format.arguments == 0 {
            self.emit_no_string_interpolation(binary.range());
            return;
        }
        if format.mapping && format.arguments > 0 {
            self.emit(
                "E1302",
                "mixed-format-string",
                "Mixing named and unnamed conversion specifiers in format string",
                binary.range(),
            );
        } else if format.mapping {
            if let Expr::Dict(dict) = binary.right.as_ref() {
                self.check_percent_mapping(dict, &format, binary);
            } else if let Some(name) = node_name(&binary.right)
                .or_else(|| matches!(binary.right.as_ref(), Expr::Tuple(_)).then_some("Tuple"))
            {
                self.emit(
                    "E1303",
                    "format-needs-mapping",
                    &format!("Expected mapping for format string, not {name}"),
                    binary.range(),
                );
            }
        } else if let Some(arguments) = self.percent_arguments(&binary.right) {
            self.check_string_arity(arguments.len(), format.arguments, binary.range());
            for (argument, conversion) in arguments.into_iter().zip(&format.conversions) {
                if let Some(argument) = argument {
                    self.check_percent_resolved(argument, *conversion, binary.range());
                }
            }
        }
    }

    fn percent_arguments(&self, expression: &Expr) -> Option<Vec<Option<Resolved>>> {
        match expression {
            Expr::Tuple(tuple) if !tuple.elts.iter().any(Expr::is_starred_expr) => Some(
                tuple
                    .elts
                    .iter()
                    .map(|value| self.expression_target(value))
                    .collect(),
            ),
            Expr::Tuple(_) => None,
            Expr::Name(_) => match self.static_value(expression) {
                Value::Sequence(values, "tuple") => Some(
                    values
                        .into_iter()
                        .map(|value| {
                            Some(Resolved {
                                target: Target::Value(Rc::new(value)),
                                context: Context::Scope(self.scopes.current_index()),
                            })
                        })
                        .collect(),
                ),
                Value::String(_)
                | Value::Bytes(_)
                | Value::Scalar(_)
                | Value::Number(_)
                | Value::Boolean(_) => Some(vec![self.expression_target(expression)]),
                _ => None,
            },
            Expr::Dict(_) | Expr::DictComp(_) => Some(vec![self.expression_target(expression)]),
            _ if node_name(expression).is_some() => Some(vec![self.expression_target(expression)]),
            _ => None,
        }
    }

    fn check_percent_mapping(&mut self, dict: &ExprDict, format: &Format, binary: &ExprBinOp) {
        let mut keys = Vec::new();
        let mut unknown = false;
        for item in &dict.items {
            match &item.key {
                Some(Expr::StringLiteral(string)) => {
                    let key = string.value.to_str().to_string();
                    if !keys.contains(&key) {
                        keys.push(key);
                    }
                }
                Some(key) if node_name(key) == Some("Const") => {
                    let value = representation::constant(key);
                    self.emit(
                        "W1300",
                        "bad-format-string-key",
                        &format!("Format string dictionary key should be a string, not {value}"),
                        binary.range(),
                    );
                }
                _ => unknown = true,
            }
        }
        if !unknown {
            for (key, _) in &format.keys {
                if !keys.contains(key) {
                    self.emit(
                        "E1304",
                        "missing-format-string-key",
                        &format!(
                            "Missing key {} in format string dictionary",
                            representation::repr(key)
                        ),
                        binary.range(),
                    );
                }
            }
        }
        for key in keys {
            if !format.keys.iter().any(|(name, _)| name == &key) {
                self.emit(
                    "W1301",
                    "unused-format-string-key",
                    &format!(
                        "Unused key {} in format string dictionary",
                        representation::repr(&key)
                    ),
                    binary.range(),
                );
            }
        }
        for item in &dict.items {
            if let Some(Expr::StringLiteral(string)) = &item.key {
                if let Some((_, conversion)) = format
                    .keys
                    .iter()
                    .find(|(key, _)| key == string.value.to_str())
                {
                    self.check_percent_type(&item.value, *conversion, binary.range());
                }
            }
        }
    }

    fn check_percent_type(
        &mut self,
        expression: &Expr,
        conversion: char,
        range: ruff_text_size::TextRange,
    ) {
        let Some(resolved) = self.expression_target(expression) else {
            return;
        };
        self.check_percent_resolved(resolved, conversion, range);
    }

    fn check_percent_resolved(
        &mut self,
        resolved: Resolved,
        conversion: char,
        range: ruff_text_size::TextRange,
    ) {
        if "sra".contains(conversion) {
            return;
        }
        let (name, bases) = match resolved.target {
            Target::Value(value) => {
                let value =
                    self.expand_value((*value).clone(), &resolved.context, &mut Default::default());
                let name = match &value {
                    Value::Sequence(_, kind) => *kind,
                    Value::Mapping(_) => "dict",
                    _ => match value.builtin_name() {
                        Some(name) => name,
                        None => return,
                    },
                };
                (format!("builtins.{name}"), vec![name.to_string()])
            }
            Target::Instance(class, context) => {
                let name = format!("{}.{}", class.identity.0, class.name);
                let bases = self
                    .lineage(class, context)
                    .into_iter()
                    .filter(|(class, _)| class.identity.0 == "builtins")
                    .map(|(class, _)| class.name.clone())
                    .collect();
                (name, bases)
            }
            _ => return,
        };
        let valid = if bases.iter().any(|name| name == "str") {
            conversion == 'c'
        } else if bases
            .iter()
            .any(|name| matches!(name.as_str(), "int" | "bool"))
        {
            true
        } else if bases.iter().any(|name| name == "float") {
            "diueEfFgGn%".contains(conversion)
        } else {
            false
        };
        if !valid {
            self.emit(
                "E1307",
                "bad-string-format-type",
                &format!(
                    "Argument {} does not match format type {}",
                    representation::repr(&name),
                    representation::repr(&conversion.to_string())
                ),
                range,
            );
        }
    }
}
