//! Default string diagnostics share syntax scanning, not logging policy.
use super::{callables::Target, resolution::Context, values::Value, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall, Operator};
use ruff_text_size::Ranged;
mod access;
mod brace;
mod percent;
mod representation;
use brace::{Format, Key};

pub(super) fn python_string_repr(text: &str) -> String {
    representation::repr(text)
}

pub(super) fn field_names(text: &str) -> Result<Vec<String>, ()> {
    brace::parse(text).map(|format| {
        format
            .fields
            .into_iter()
            .map(|field| match field.key {
                Key::Name(name) => name,
                Key::Index(index) => index.to_string(),
            })
            .collect()
    })
}

impl Checker<'_> {
    pub(super) fn check_string_format(&mut self, expression: &Expr) {
        match expression {
            Expr::BinOp(binary) if binary.op == Operator::Mod => self.check_percent_string(binary),
            Expr::Call(call) => self.check_string_call(call),
            _ => {}
        }
    }

    fn string_method(&self, expression: &Expr) -> Option<(String, Value)> {
        if let Expr::Attribute(attribute) = expression {
            return Some((
                attribute.attr.to_string(),
                self.static_value(&attribute.value),
            ));
        }
        let resolved = self.expression_target(expression)?;
        let Target::Value(value) = resolved.target else {
            return None;
        };
        let Value::StringMethod(name, bound) =
            self.expand_value((*value).clone(), &resolved.context, &mut Default::default())
        else {
            return None;
        };
        Some((name, *bound))
    }

    fn check_string_call(&mut self, call: &ExprCall) {
        let Some((name, value)) = self.string_method(&call.func) else {
            return;
        };
        if matches!(name.as_str(), "strip" | "lstrip" | "rstrip") {
            if let (Some(bound), Some(argument)) =
                (value.builtin_name(), call.arguments.args.first())
            {
                if matches!(bound, "str" | "bytes") {
                    if let Value::String(text) = self.static_value(argument) {
                        if text.chars().count()
                            != text
                                .chars()
                                .collect::<std::collections::BTreeSet<_>>()
                                .len()
                        {
                            self.emit(
                                "E1310",
                                "bad-str-strip-call",
                                &format!("Suspicious argument in {bound}.{name} call"),
                                call.range(),
                            );
                        }
                    }
                }
            }
            return;
        }
        if name != "format"
            || matches!(call.func.as_ref(), Expr::Attribute(attribute) if !matches!(attribute.value.as_ref(), Expr::StringLiteral(_)))
            || call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            return;
        }
        let Value::String(text) = value else { return };
        let Ok(format) = brace::parse(&text) else {
            self.emit(
                "W1302",
                "bad-format-string",
                "Invalid format string",
                call.range(),
            );
            return;
        };
        self.check_brace_arguments(call, &format);
    }

    fn check_brace_arguments(&mut self, call: &ExprCall, format: &Format) {
        if format.implicit > 0 && format.manual > 0 {
            self.emit("W1305", "format-combined-specification", "Format string contains both automatic field numbering and manual field specification", call.range());
            return;
        }
        let mut names = Vec::new();
        for field in &format.fields {
            if let Key::Name(name) = &field.key {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        let mut required = format.implicit + usize::from(names.iter().any(String::is_empty));
        let has_keywords = !call.arguments.keywords.is_empty();
        let empty = names.iter().any(String::is_empty);
        for name in &names {
            if !name.is_empty()
                && !call
                    .arguments
                    .keywords
                    .iter()
                    .any(|keyword| keyword.arg.as_ref().is_some_and(|arg| arg.as_str() == name))
            {
                self.emit(
                    "W1303",
                    "missing-format-argument-key",
                    &format!(
                        "Missing keyword argument {} for format string",
                        representation::repr(name)
                    ),
                    call.range(),
                );
            }
        }
        if !names.is_empty() {
            for keyword in &call.arguments.keywords {
                if let Some(name) = &keyword.arg {
                    if !names.iter().any(|field| field == name.as_str()) {
                        self.emit(
                            "W1304",
                            "unused-format-string-argument",
                            &format!(
                                "Unused format argument {}",
                                representation::repr(name.as_str())
                            ),
                            call.range(),
                        );
                    }
                }
            }
            if required == 0 {
                required = format.manual;
            }
        }
        let check = names.is_empty()
            || ((!call.arguments.args.is_empty() || required > 0) && (has_keywords || empty));
        if check {
            if required == 0 {
                required = format.manual;
            }
            if required == 0 {
                self.emit_no_string_interpolation(call.range());
                return;
            }
            self.check_string_arity(call.arguments.args.len(), required, call.range());
        }
        let mut duplicates = Vec::<(String, usize)>::new();
        for argument in &call.arguments.args {
            if let Expr::Name(name) = argument {
                if let Some((_, count)) = duplicates
                    .iter_mut()
                    .find(|(value, _)| value == name.id.as_str())
                {
                    *count += 1;
                } else {
                    duplicates.push((name.id.to_string(), 1));
                }
            }
        }
        for (name, count) in duplicates {
            if count > 1 {
                self.emit("W1308", "duplicate-string-formatting-argument", &format!("Duplicate string formatting argument {}, consider passing as named argument", representation::repr(&name)), call.range());
            }
        }
        self.check_format_access(call, &format.fields);
    }

    fn check_string_arity(
        &mut self,
        supplied: usize,
        required: usize,
        range: ruff_text_size::TextRange,
    ) {
        if supplied > required {
            self.emit(
                "E1305",
                "too-many-format-args",
                "Too many arguments for format string",
                range,
            );
        } else if supplied < required {
            self.emit(
                "E1306",
                "too-few-format-args",
                "Not enough arguments for format string",
                range,
            );
        }
    }

    fn emit_no_string_interpolation(&mut self, range: ruff_text_size::TextRange) {
        self.emit(
            "W1310",
            "format-string-without-interpolation",
            "Using formatting for a string that does not have any interpolated variables",
            range,
        );
    }
}
