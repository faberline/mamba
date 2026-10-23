//! Base diagnostics keep semantic errors separate from runtime member inference.
use super::*;
use quality_syntax::python_ast::StmtClassDef;
use ruff_text_size::TextRange;
use types::Type;

impl Checker<'_> {
    pub(super) fn check_bases(&mut self, statement: &StmtClassDef) {
        let Some(class) = self.scopes.class_object(statement.name.as_str()) else {
            return;
        };
        let Some(id) = self.context.class_id(class) else {
            return;
        };
        let class = self.context.class(id);
        let variables = self
            .context
            .class_scope_variables(id, self.scopes.variables());
        if class.plugin {
            return;
        }
        if let Some(arguments) = &statement.arguments {
            for base in &arguments.args {
                if matches!(base, Expr::Name(name) if name.id.as_str() == "type" && !self.scopes.has_binding("type"))
                {
                    continue;
                }
                let form = if let Expr::Subscript(base) = base {
                    base.value.as_ref()
                } else {
                    base
                };
                if matches!(
                    self.context
                        .typing_form(form, &self.scopes.variables())
                        .as_deref(),
                    Some("Protocol" | "Generic")
                ) {
                    continue;
                }
                let range = if matches!(base, Expr::Subscript(_)) {
                    base.range()
                } else {
                    TextRange::at(base.start(), ruff_text_size::TextSize::new(1))
                };
                if !self.check_class_annotation_at(base, range)
                    && matches!(base, Expr::Subscript(_))
                {
                    self.check_callable_annotation(base);
                }
                match self.context.annotation_with_variables(base, &variables) {
                    Some(ty) if self.context.class_id(ty).is_some() => {
                        let parent = self.context.class_id(ty).unwrap();
                        if self.context.class(parent).final_class {
                            self.emit(
                                "misc",
                                format!(
                                    "Cannot inherit from final class \"{}\"",
                                    self.context.class(parent).name
                                ),
                                statement,
                            );
                        }
                        if (self.context.has_base(parent, id) || parent == id)
                            && self.context.class(parent).offset >= class.offset
                        {
                            self.cyclic_base(base);
                        }
                    }
                    Some(Type::Bool) => self.emit(
                        "misc",
                        "Cannot inherit from final class \"bool\"".into(),
                        statement,
                    ),
                    Some(Type::Any | Type::AnySpecial) => self.any_base(base),
                    Some(
                        Type::Int
                        | Type::Float
                        | Type::Complex
                        | Type::Str
                        | Type::Bytes
                        | Type::Object,
                    ) => {}
                    _ => self.invalid_base(base),
                }
            }
        }
        match self.context.class_mro(id) {
            Err(super::super::classes::mro::Error::Duplicate(base)) => {
                self.emit(
                    "misc",
                    format!("Duplicate base class \"{}\"", self.context.class(base).name),
                    statement,
                );
            }
            Err(super::super::classes::mro::Error::Inconsistent) => self.emit(
                "misc",
                format!(
                    "Cannot determine consistent method resolution order (MRO) for \"{}\"",
                    class.name
                ),
                statement,
            ),
            _ => {}
        }
    }

    fn cyclic_base(&mut self, base: &Expr) {
        if let Some(name) = base_name(base) {
            self.emit(
                "misc",
                format!("Cannot resolve name \"{name}\" (possible cyclic definition)"),
                base,
            );
        }
        self.any_base(base);
    }

    fn any_base(&mut self, base: &Expr) {
        let message = base_name(base).map_or_else(
            || "Class cannot subclass value of type \"Any\"".into(),
            |name| format!("Class cannot subclass \"{name}\" (has type \"Any\")"),
        );
        self.emit("misc", message, base);
    }

    fn invalid_base(&mut self, base: &Expr) {
        let point = TextRange::new(
            base.start(),
            base.start() + ruff_text_size::TextSize::new(1),
        );
        if let Expr::NumberLiteral(number) = base {
            let message = match &number.value {
                quality_syntax::python_ast::Number::Int(number) => {
                    format!("Invalid type: try using Literal[{number}] instead?")
                }
                quality_syntax::python_ast::Number::Float(_) => {
                    "Invalid type: float literals cannot be used as a type".into()
                }
                quality_syntax::python_ast::Number::Complex { .. } => {
                    "Invalid type: complex literals cannot be used as a type".into()
                }
            };
            self.emit("valid-type", message, &point);
            self.any_base(base);
            return;
        }
        if let Expr::Name(name) = base {
            if self.scopes.resolved_binding(name.id.as_str()).is_none() {
                self.emit(
                    "name-defined",
                    format!("Name \"{}\" is not defined", name.id),
                    base,
                );
                self.any_base(base);
                return;
            }
            if self.infer(base).is_some_and(Type::is_any) {
                self.any_base(base);
                return;
            }
            let function = self.scopes.callable(name.id.as_str()).is_some();
            let kind = if function { "Function" } else { "Variable" };
            let hint = if function {
                "Perhaps you need \"Callable[...]\" or a callback protocol?"
            } else {
                "See https://mypy.readthedocs.io/en/stable/common_issues.html#variables-vs-type-aliases"
            };
            self.emit_with_hint(
                "valid-type",
                format!(
                    "{kind} \"{}\" is not valid as a type",
                    self.scopes.qualified_name(name.id.as_str())
                ),
                &point,
                Some(hint.into()),
            );
        }
        let (kind, name) = if let Expr::Call(call) = base {
            ("Unsupported dynamic base class", base_name(&call.func))
        } else {
            ("Invalid base class", base_name(base))
        };
        let message = name.map_or_else(|| kind.into(), |name| format!("{kind} \"{name}\""));
        self.emit("misc", message, base);
    }
}

fn base_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        _ => None,
    }
}
