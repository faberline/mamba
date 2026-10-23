//! Standard-library call warnings preserve the original raw argument gates.
use super::{operand_inference::Operand, Checker};
use mamba_language::application::quality_syntax::python_ast::{Expr, ExprCall};
use ruff_text_size::Ranged;
impl Checker<'_> {
    pub(super) fn check_stdlib_call(&mut self, call: &ExprCall) {
        self.check_unittest_call(call);
        let Some(name) = self.stdlib_name(&call.func) else {
            return;
        };
        match name.as_str() {
            "threading.Thread"
                if call.arguments.args.len() < 2
                    && !call.arguments.keywords.iter().any(|value| {
                        value
                            .arg
                            .as_ref()
                            .is_some_and(|name| name.as_str() == "target")
                    }) =>
            {
                self.emit(
                    "W1506",
                    "bad-thread-instantiation",
                    "threading.Thread needs the target function",
                    call.range(),
                )
            }
            "subprocess.Popen" => {
                if call.arguments.keywords.iter().any(|value| {
                    value
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "preexec_fn")
                }) {
                    self.emit(
                        "W1509",
                        "subprocess-popen-preexec-fn",
                        "Using preexec_fn keyword which may be unsafe in the presence of threads",
                        call.range(),
                    );
                }
            }
            "subprocess.run" => {
                if !call.arguments.keywords.iter().any(|value| {
                    value
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "check")
                }) {
                    self.emit(
                        "W1510",
                        "subprocess-run-check",
                        "'subprocess.run' used without explicitly defining the value for 'check'.",
                        call.range(),
                    );
                }
            }
            "builtins.breakpoint" | "sys.breakpointhook" | "pdb.set_trace" => self.emit(
                "W1515",
                "forgotten-debug-statement",
                "Leaving functions creating breakpoints in production code is not recommended",
                call.range(),
            ),
            "os.getenv" => self.check_getenv(call),
            "copy.copy" => {
                if let Some(argument) = self.stdlib_argument(call, 0, "x") {
                    if self
                        .operand_nodes(&argument.0, argument.1)
                        .iter()
                        .any(|(value, origin)| {
                            self.stdlib_name_in(
                                value,
                                origin.clone(),
                                &mut std::collections::BTreeSet::new(),
                            )
                            .as_deref()
                                == Some("os.environ")
                        })
                    {
                        self.emit(
                            "W1507",
                            "shallow-copy-environ",
                            "Using copy.copy(os.environ). Use os.environ.copy() instead.",
                            call.range(),
                        );
                    }
                }
            }
            "builtins.open" | "io.open" | "_io.open" => self.check_open_arguments(call),
            name if name.starts_with("pathlib.")
                && name
                    .rsplit('.')
                    .next()
                    .is_some_and(|name| matches!(name, "open" | "read_text" | "write_text")) =>
            {
                self.check_open_arguments(call)
            }
            _ => {}
        }
    }
    fn check_getenv(&mut self, call: &ExprCall) {
        let key = call.arguments.args.first().cloned().or_else(|| {
            call.arguments
                .keywords
                .iter()
                .find(|value| {
                    value
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "key")
                })
                .map(|value| value.value.clone())
        });
        if let Some(value) = key {
            if let Some(name) = self
                .stdlib_type(&value)
                .filter(|name| name != "builtins.str")
            {
                self.emit(
                    "E1507",
                    "invalid-envvar-value",
                    &format!("os.getenv does not support {name} type argument"),
                    call.range(),
                );
            }
        }
        let default = if call.arguments.args.len() == 2 {
            call.arguments.args.get(1).cloned()
        } else {
            call.arguments
                .keywords
                .iter()
                .find(|value| {
                    value
                        .arg
                        .as_ref()
                        .is_some_and(|name| name.as_str() == "default")
                })
                .map(|value| value.value.clone())
        };
        if let Some(value) = default {
            if let Some(name) = self
                .stdlib_type(&value)
                .filter(|name| !matches!(name.as_str(), "builtins.str" | "builtins.NoneType"))
            {
                self.emit(
                    "W1508",
                    "invalid-envvar-default",
                    &format!("os.getenv default type is {name}. Expected str or None."),
                    call.range(),
                );
            }
        }
    }
    fn stdlib_type(&self, value: &Expr) -> Option<String> {
        match self.operand(value) {
            Operand::Literal(name) => Some(format!("builtins.{name}")),
            Operand::Class(_, _) => Some("builtins.type".into()),
            Operand::Instance(class, _) => Some(format!("{}.{}", class.identity.0, class.name)),
            Operand::Function(_) => Some("builtins.function".into()),
            Operand::Generator => Some("builtins.generator".into()),
            Operand::Unknown => None,
        }
    }
}
