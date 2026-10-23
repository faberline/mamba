//! Tuple variable declarations retain original errors and default recovery.
use super::*;
use quality_syntax::python_ast::{StmtAssign, TypeParam, TypeParams};

impl Checker<'_> {
    pub(in crate::infrastructure::mypy::checker) fn check_tuple_variable_defaults(
        &mut self,
        parameters: Option<&TypeParams>,
    ) {
        for parameter in parameters
            .into_iter()
            .flat_map(|parameters| &parameters.type_params)
        {
            if let TypeParam::TypeVarTuple(parameter) = parameter {
                if let Some(default) = parameter.default.as_deref() {
                    self.check_callable_annotation(default);
                }
            }
        }
    }

    pub(in crate::infrastructure::mypy::checker) fn check_tuple_variable_legacy(
        &mut self,
        assignment: &StmtAssign,
    ) {
        let [Expr::Name(name)] = assignment.targets.as_slice() else {
            return;
        };
        let Expr::Call(call) = assignment.value.as_ref() else {
            return;
        };
        if self
            .context
            .typing_form(&call.func, &self.scopes.variables())
            .as_deref()
            != Some("TypeVarTuple")
        {
            return;
        }
        match call.arguments.args.first() {
            None => {
                self.emit(
                    "misc",
                    "Too few arguments for TypeVarTuple()".into(),
                    assignment,
                );
                self.emit(
                    "call-arg",
                    "Missing positional argument \"name\" in call to \"TypeVarTuple\"".into(),
                    call,
                );
            }
            Some(Expr::StringLiteral(string)) if string.value.to_str() != name.id.as_str() => {
                self.emit("misc", format!("String argument 1 \"{}\" to TypeVarTuple(...) does not match variable name \"{}\"", string.value.to_str(), name.id), assignment);
            }
            Some(Expr::StringLiteral(_)) => {}
            Some(value) => {
                self.emit(
                    "misc",
                    "TypeVarTuple() expects a string literal as first argument".into(),
                    assignment,
                );
                if let Some(actual) = self
                    .infer(value)
                    .filter(|&actual| actual != types::Type::Str && !actual.is_any())
                {
                    self.emit("arg-type", format!("Argument 1 to \"TypeVarTuple\" has incompatible type \"{}\"; expected \"str\"", self.context.name(actual)), value);
                }
            }
        }
        if call.arguments.args.len() > 1 {
            self.emit(
                "misc",
                "Too many positional arguments for \"TypeVarTuple\"".into(),
                assignment,
            );
        }
        for keyword in &call.arguments.keywords {
            match keyword.arg.as_ref().map(|name| name.as_str()) {
                Some("default") => {
                    let value = &keyword.value;
                    if value.is_ellipsis_literal_expr() {
                        self.emit_column_zero("misc", "Unexpected \"...\"".into(), assignment);
                    } else if let Expr::List(_) = value {
                        self.emit(
                            "valid-type",
                            "Bracketed expression \"[...]\" is not valid as a type".into(),
                            &ruff_text_size::TextRange::at(
                                value.start(),
                                ruff_text_size::TextSize::new(1),
                            ),
                        );
                    }
                    if !self
                        .scopes
                        .annotation(value)
                        .is_some_and(|ty| self.context.unpack_target(ty).is_some())
                    {
                        self.emit(
                            "misc",
                            "The default argument to TypeVarTuple must be an Unpacked tuple".into(),
                            value,
                        );
                    } else {
                        self.check_callable_annotation(value);
                    }
                }
                Some(name) => self.emit(
                    "misc",
                    format!("Unexpected keyword argument \"{name}\" for \"TypeVarTuple\""),
                    assignment,
                ),
                None => {}
            }
        }
    }
}
