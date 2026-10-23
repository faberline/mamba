//! Default logging rules keep module-call and bound Logger inference separate.
use super::{
    callables::Target, directives::suppressed, resolution::Context, values::Value, Checker,
};
use mamba_language::application::quality_syntax::python_ast::{
    Expr, ExprCall, InterpolatedStringElement, Operator,
};
use ruff_text_size::Ranged;
pub(super) mod percent;
pub(super) mod stdlib;
mod strings;

fn convenience(name: &str) -> bool {
    matches!(
        name,
        "critical" | "debug" | "error" | "exception" | "fatal" | "info" | "warn" | "warning"
    )
}

fn explicit_string(expression: &Expr) -> bool {
    match expression {
        Expr::StringLiteral(_) => true,
        Expr::BinOp(binary) if binary.op == Operator::Add => {
            explicit_string(&binary.left) && explicit_string(&binary.right)
        }
        _ => false,
    }
}

impl Checker<'_> {
    pub(super) fn logging_method(&self, expression: &Expr) -> Option<String> {
        if let Expr::Attribute(attribute) = expression {
            if matches!(attribute.value.as_ref(), Expr::Name(name) if self.logging_names.contains(name.id.as_str()))
            {
                return Some(attribute.attr.to_string());
            }
        }
        let Target::Function(signature) = self.callable_target(expression)? else {
            return None;
        };
        if signature.category != "method" {
            return None;
        }
        let (module, name) = signature.owner.as_ref()?;
        let Target::Class(class) = self
            .modules
            .call_target(self.source.project, module, name)?
        else {
            return None;
        };
        self.lineage(class, Context::Module(module.clone()))
            .iter()
            .any(|(class, _)| class.identity.0 == "logging" && class.name == "Logger")
            .then(|| signature.name.clone())
    }

    fn logging_warning(&mut self, rule: &str, name: &str, call: &ExprCall) {
        let line = self.source.text[..usize::from(call.start())]
            .bytes()
            .filter(|&byte| byte == b'\n')
            .count() as u32
            + 1;
        let mut permitted = vec!["lazy %"];
        // The locked original queries the unregistered
        // logging-fstring-formatting name, which remains enabled.
        if suppressed(
            &self.directives,
            "W1202",
            "logging-format-interpolation",
            line,
        ) {
            permitted.push(".format()");
        }
        if suppressed(&self.directives, "W1201", "logging-not-lazy", line) {
            permitted.push("%");
        }
        self.emit(
            rule,
            name,
            &format!(
                "Use {} formatting in logging functions",
                permitted.join(" or ")
            ),
            call.range(),
        );
    }

    pub(super) fn check_logging(&mut self, call: &ExprCall) {
        let Some(name) = self.logging_method(&call.func) else {
            return;
        };
        if name == "warn" {
            self.emit(
                "W4902",
                "deprecated-method",
                "Using deprecated method warn()",
                call.range(),
            );
        }
        let position = if name == "log" {
            1
        } else if convenience(&name) {
            0
        } else {
            return;
        };
        if call.arguments.args.len() <= position
            || call.arguments.args.iter().any(Expr::is_starred_expr)
            || call
                .arguments
                .keywords
                .iter()
                .any(|keyword| keyword.arg.is_none())
        {
            return;
        }
        match &call.arguments.args[position] {
            Expr::BinOp(binary) => {
                if binary.op == Operator::Mod
                    || (binary.op == Operator::Add
                        && !explicit_string(&call.arguments.args[position])
                        && [&binary.left, &binary.right]
                            .iter()
                            .any(|value| matches!(self.static_value(value), Value::String(_))))
                {
                    self.logging_warning("W1201", "logging-not-lazy", call);
                }
            }
            Expr::Call(formatted) => {
                if let Expr::Attribute(attribute) = formatted.func.as_ref() {
                    if attribute.attr.as_str() == "format" {
                        if let Value::String(text) = self.static_value(&attribute.value) {
                            if !strings::complex(&text) {
                                self.logging_warning(
                                    "W1202",
                                    "logging-format-interpolation",
                                    formatted,
                                );
                            }
                        }
                    }
                }
            }
            Expr::FString(string) => {
                let percent_format = string.value.elements().any(|element| {
                    matches!(element, InterpolatedStringElement::Literal(literal) if ["%s", "%d", "%f", "%r"].iter().any(|format| literal.value.contains(format)))
                });
                if !percent_format {
                    self.logging_warning("W1203", "logging-fstring-interpolation", call);
                }
            }
            Expr::StringLiteral(string) => {
                self.logging_format(string.value.to_str(), call, position)
            }
            Expr::BytesLiteral(bytes) => {
                let bytes: Vec<_> = bytes.value.bytes().collect();
                if let Ok(text) = std::str::from_utf8(&bytes) {
                    self.logging_format(text, call, position);
                }
            }
            Expr::NoneLiteral(_) | Expr::BooleanLiteral(_) | Expr::NumberLiteral(_)
                if call.arguments.args.len() > position + 1 =>
            {
                self.emit(
                    "E1205",
                    "logging-too-many-args",
                    "Too many arguments for logging format string",
                    call.range(),
                );
            }
            _ => {}
        }
    }

    fn logging_format(&mut self, text: &str, call: &ExprCall, position: usize) {
        let supplied = call.arguments.args.len() - position - 1;
        match percent::parse(text) {
            Err(percent::Error::Truncated) => self.emit(
                "E1201",
                "logging-format-truncated",
                "Logging format string ends in middle of conversion specifier",
                call.range(),
            ),
            Err(percent::Error::Unsupported(character, index)) if supplied > 0 => self.emit(
                "E1200",
                "logging-unsupported-format",
                &format!(
                    "Unsupported logging format character {} ({:#x}) at index {index}",
                    percent::repr(character),
                    u32::from(character)
                ),
                call.range(),
            ),
            Err(_) => {}
            Ok(format) if format.mapping => {}
            Ok(format) if supplied > format.arguments => self.emit(
                "E1205",
                "logging-too-many-args",
                "Too many arguments for logging format string",
                call.range(),
            ),
            Ok(format) if supplied < format.arguments => self.emit(
                "E1206",
                "logging-too-few-args",
                "Not enough arguments for logging format string",
                call.range(),
            ),
            _ => {}
        }
    }
}
