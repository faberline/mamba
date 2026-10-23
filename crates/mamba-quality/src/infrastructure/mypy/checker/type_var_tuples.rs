//! Tuple variable errors keep the declaration scope and unpack source span.
use super::*;
mod declarations;

impl Checker<'_> {
    fn tuple_variable_bound(&self, variable: types::Type) -> bool {
        self.scopes.type_variable_bound(variable)
            || self.class_types.iter().flatten().any(|&class| {
                self.context
                    .class_id(class)
                    .is_some_and(|id| self.context.class(id).generic.contains(&variable))
            })
    }

    pub(super) fn check_tuple_variable_annotation(&mut self, expression: &Expr) -> bool {
        if let Expr::Starred(starred) = expression {
            self.check_tuple_unpack_target(&starred.value);
            return true;
        }
        if let Expr::Subscript(subscript) = expression {
            if self
                .context
                .typing_form(&subscript.value, &self.scopes.variables())
                .as_deref()
                == Some("Unpack")
            {
                self.check_tuple_unpack_target(&subscript.slice);
                return true;
            }
            let name = match subscript.value.as_ref() {
                Expr::Name(name) => name.id.as_str(),
                Expr::Attribute(attribute) => attribute.attr.as_str(),
                _ => "",
            };
            let arguments = match subscript.slice.as_ref() {
                Expr::Tuple(tuple) => tuple.elts.iter().collect::<Vec<_>>(),
                expression => vec![expression],
            };
            if matches!(name, "tuple" | "Tuple") {
                let mut unpacked = None;
                for argument in &arguments {
                    let target = match argument {
                        Expr::Starred(starred) => Some(starred.value.as_ref()),
                        Expr::Subscript(unpack)
                            if self
                                .context
                                .typing_form(&unpack.value, &self.scopes.variables())
                                .as_deref()
                                == Some("Unpack") =>
                        {
                            Some(unpack.slice.as_ref())
                        }
                        _ => None,
                    };
                    if let Some(target) = target.filter(|target| {
                        self.scopes.annotation(target).is_some_and(|ty| {
                            self.context.is_tuple_variable(ty)
                                || self.context.node(ty).is_some_and(|node| {
                                    matches!(node.as_ref(), types::Node::VariadicTuple(_))
                                })
                        })
                    }) {
                        if unpacked.is_some() {
                            self.emit(
                                "misc",
                                "More than one variadic Unpack in a type is not allowed".into(),
                                &ruff_text_size::TextRange::at(
                                    target.start(),
                                    ruff_text_size::TextSize::new(1),
                                ),
                            );
                        }
                        unpacked = Some(target);
                    }
                }
            }
            if matches!(name, "list" | "List")
                && arguments.iter().any(|argument| {
                    self.scopes
                        .annotation(argument)
                        .is_some_and(|ty| self.context.unpack_target(ty).is_some())
                })
            {
                self.emit(
                    "valid-type",
                    "Unpack is only valid in a variadic position".into(),
                    expression,
                );
                self.emit(
                    "type-arg",
                    "Missing type arguments for generic type \"list\"".into(),
                    expression,
                );
                self.check_callable_annotation(&subscript.slice);
                return true;
            }
        }
        let Some(variable) = self
            .scopes
            .annotation(expression)
            .filter(|&ty| self.context.is_tuple_variable(ty))
        else {
            return false;
        };
        let message = if self.tuple_variable_bound(variable) {
            format!(
                "TypeVarTuple \"{}\" is only valid with an unpack",
                self.context.name(variable)
            )
        } else {
            format!(
                "TypeVarTuple \"{}\" is unbound",
                self.context.name(variable)
            )
        };
        self.emit("valid-type", message, expression);
        true
    }

    fn check_tuple_unpack_target(&mut self, expression: &Expr) {
        if let Some(variable) = self
            .scopes
            .annotation(expression)
            .filter(|&ty| self.context.is_tuple_variable(ty))
        {
            if !self.tuple_variable_bound(variable) {
                self.emit(
                    "valid-type",
                    format!(
                        "TypeVarTuple \"{}\" is unbound",
                        self.context.name(variable)
                    ),
                    expression,
                );
            }
        } else {
            if let Some(ty) = self.scopes.annotation(expression) {
                if !ty.is_any()
                    && !self.context.node(ty).is_some_and(|node| {
                        matches!(
                            node.as_ref(),
                            types::Node::Tuple(_) | types::Node::VariadicTuple(_)
                        )
                    })
                {
                    self.emit(
                        "valid-type",
                        format!(
                            "\"{}\" cannot be unpacked (must be tuple or TypeVarTuple)",
                            self.context.name(ty)
                        ),
                        &ruff_text_size::TextRange::at(
                            expression.start(),
                            ruff_text_size::TextSize::new(1),
                        ),
                    );
                    return;
                }
            }
            self.check_callable_annotation(expression);
        }
    }
}
