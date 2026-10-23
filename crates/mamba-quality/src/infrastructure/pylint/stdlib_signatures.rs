//! Selected standard-library call contracts retain original diagnostic order.
use super::{arguments::Arguments, Checker};
use mamba_language::application::quality_syntax::python_ast::ExprCall;
use ruff_text_size::Ranged;
impl Checker<'_> {
    pub(super) fn check_stdlib_signature(&mut self, call: &ExprCall) {
        let Some(name) = self.stdlib_name(&call.func) else {
            return;
        };
        if name == "os.getenv" {
            if matches!(
                self.callable_target(&call.func),
                Some(super::callables::Target::Function(_))
            ) {
                return;
            }
            let arguments = Arguments::from_call(self, call);
            if arguments.invalid {
                return;
            }
            for key in &arguments.keywords {
                let position = match key.as_str() {
                    "key" => Some(0),
                    "default" => Some(1),
                    _ => None,
                };
                if let Some(position) = position {
                    if position < arguments.positional {
                        self.emit(
                            "E1124",
                            "redundant-keyword-arg",
                            &format!(
                                "Argument '{key}' passed by position and keyword in function call"
                            ),
                            call.range(),
                        );
                    }
                } else {
                    self.emit(
                        "E1123",
                        "unexpected-keyword-arg",
                        &format!("Unexpected keyword argument '{key}' in function call"),
                        call.range(),
                    );
                }
            }
            if arguments.positional > 2 {
                self.emit(
                    "E1121",
                    "too-many-function-args",
                    "Too many positional arguments for function call",
                    call.range(),
                );
            } else if arguments.positional == 0
                && !arguments.might_supply_keywords
                && !arguments.keywords.iter().any(|name| name == "key")
            {
                self.emit(
                    "E1120",
                    "no-value-for-parameter",
                    "No value for argument 'key' in function call",
                    call.range(),
                );
            }
        } else if name.starts_with("pathlib.")
            && (name.ends_with(".read_text") || name.ends_with(".write_text"))
        {
            for keyword in &call.arguments.keywords {
                if let Some(name) = &keyword.arg {
                    if name.as_str() == "mode" {
                        self.emit(
                            "E1123",
                            "unexpected-keyword-arg",
                            "Unexpected keyword argument 'mode' in method call",
                            call.range(),
                        );
                    }
                }
            }
        }
    }
}
