//! Raising checks retain reference spelling, inferred type, and cause ordering.
use super::{exception_values::Inferred, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, StmtRaise};
use ruff_text_size::Ranged;

impl Checker<'_> {
    pub(super) fn check_exception_raise(&mut self, raised: &StmtRaise) {
        let Some(value) = &raised.exc else {
            return;
        };
        if let Some(cause) = &raised.cause {
            let invalid = match self.exception_value(cause) {
                Inferred::Class(class, context, true) => {
                    !self.standard_exception(class.clone(), context.clone())
                        && self.known_exception_bases(class, context)
                }
                Inferred::Class(_, _, false) | Inferred::Unknown | Inferred::Scalar("NoneType") => {
                    false
                }
                _ => true,
            };
            if invalid {
                self.emit(
                    "E0705",
                    "bad-exception-cause",
                    "Exception cause set to something which is not an exception, nor None",
                    raised.range(),
                );
            }
        }
        let reference = match value.as_ref() {
            Expr::Name(_) => Some(value.as_ref()),
            Expr::Call(call) if matches!(call.func.as_ref(), Expr::Name(_)) => {
                Some(call.func.as_ref())
            }
            _ => None,
        };
        if let Some(reference) = reference {
            if matches!(reference, Expr::Name(name) if name.id.as_str() == "NotImplemented") {
                self.emit(
                    "E0711",
                    "notimplemented-raised",
                    "NotImplemented raised - should raise NotImplementedError",
                    raised.range(),
                );
            } else if let Inferred::Class(class, _, _) = self.exception_value(reference) {
                if class.identity.0 == "builtins"
                    && matches!(class.name.as_str(), "Exception" | "BaseException")
                {
                    self.emit(
                        "W0719",
                        "broad-exception-raised",
                        &format!("Raising too general exception: {}", class.name),
                        raised.range(),
                    );
                }
            }
        }
        if let Expr::Call(call) = value.as_ref() {
            if call.arguments.args.len() >= 2 {
                if let Expr::StringLiteral(message) = &call.arguments.args[0] {
                    let message = message.value.to_str();
                    if message.contains('%') || (message.contains('{') && message.contains('}')) {
                        self.emit(
                            "W0715",
                            "raising-format-tuple",
                            "Exception arguments suggest string formatting might be intended",
                            raised.range(),
                        );
                    }
                }
            }
        }
        match self.exception_value(value) {
            Inferred::Class(class, context, _) => {
                if !self.standard_exception(class.clone(), context.clone())
                    && self.known_exception_bases(class, context)
                {
                    self.emit(
                        "E0710",
                        "raising-non-exception",
                        "Raising a class which doesn't inherit from BaseException",
                        raised.range(),
                    );
                }
            }
            Inferred::Scalar(name) => self.emit_bad_raise(raised, name),
            Inferred::Function(name) => self.emit_bad_raise(raised, &name),
            Inferred::Unknown => {}
        }
    }

    fn emit_bad_raise(&mut self, raised: &StmtRaise, name: &str) {
        self.emit(
            "E0702",
            "raising-bad-type",
            &format!("Raising {name} while only classes or instances are allowed"),
            raised.range(),
        );
    }
}
