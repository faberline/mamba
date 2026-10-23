//! Every class-object union branch must accept the supplied constructor call.
use super::*;
use quality_syntax::python_ast::{ExprCall, StmtClassDef, StmtFunctionDef};

impl Checker<'_> {
    pub(super) fn check_class_index(
        &mut self,
        subscript: &quality_syntax::python_ast::ExprSubscript,
    ) {
        let Some(receiver) = self.infer(&subscript.value) else {
            return;
        };
        if self.context.invalid_class_index(receiver) {
            self.emit(
                "misc",
                format!(
                    "The type \"{}\" is not generic and not indexable",
                    self.context.name(receiver)
                ),
                subscript,
            );
        }
    }

    pub(super) fn check_new_return(
        &mut self,
        function: &StmtFunctionDef,
        signature: &signatures::Signature,
    ) {
        if !self.in_class || function.name.as_str() != "__new__" || !signature.return_explicit {
            return;
        }
        let Some((id, returned)) = self
            .class_types
            .last()
            .copied()
            .flatten()
            .and_then(|ty| self.context.class_id(ty))
            .zip(signature.returns)
        else {
            return;
        };
        let expected = self.context.applied_class(
            id,
            vec![types::Type::Any; self.context.class(id).generic.len()],
            false,
        );
        if self.context.accepts(expected, returned) {
            return;
        }
        let message = if returned == types::Type::None || self.context.is_class_value(returned) {
            format!(
                "\"__new__\" must return a class instance (got \"{}\")",
                self.context.name(returned)
            )
        } else {
            format!("Incompatible return type for \"__new__\" (returns \"{}\", but must return a subtype of \"{}\")", self.context.name(returned), self.context.name(expected))
        };
        self.emit("misc", message, function);
    }

    pub(super) fn check_class_import_redefinition(&mut self, class: &StmtClassDef) -> bool {
        if !self.scopes.import_redefinition(class.name.as_str()) {
            return false;
        }
        self.emit(
            "no-redef",
            format!(
                "Name \"{}\" already defined (possibly by an import)",
                class.name
            ),
            class,
        );
        true
    }

    pub(super) fn check_type_object_annotation(&mut self, expression: &Expr) -> bool {
        let variables = self.scopes.variables();
        if matches!(expression, Expr::Name(_) | Expr::Attribute(_))
            && self.context.typing_form(expression, &variables).as_deref() == Some("Type")
        {
            let name = match expression {
                Expr::Name(name) => name.id.as_str(),
                Expr::Attribute(attribute) => attribute.attr.as_str(),
                _ => unreachable!(),
            };
            self.emit(
                "type-arg",
                format!("Missing type arguments for generic type \"{name}\""),
                expression,
            );
            return true;
        }
        if let Expr::Subscript(subscript) = expression {
            let type_object = matches!(subscript.value.as_ref(), Expr::Name(name) if name.id.as_str() == "type")
                || self
                    .context
                    .typing_form(&subscript.value, &variables)
                    .as_deref()
                    == Some("Type");
            if type_object
                && self
                    .scopes
                    .annotation(&subscript.slice)
                    .and_then(|ty| self.context.node(ty))
                    .is_some_and(|node| {
                        matches!(
                            node.as_ref(),
                            types::Node::Container(types::Container::TypeObject, _)
                        )
                    })
            {
                self.emit(
                    "valid-type",
                    "type[...] can't contain \"Type[...]\"".into(),
                    expression,
                );
                self.check_callable_annotation(&subscript.slice);
                return true;
            }
        }
        false
    }

    pub(super) fn check_type_object_call(
        &mut self,
        call: &ExprCall,
        untyped_call_policy: functions::UntypedCallPolicy,
    ) -> bool {
        let Some(signatures) = self.infer(&call.func).and_then(|ty| {
            self.context
                .type_object_constructors_for(ty, self.python_target)
        }) else {
            return false;
        };
        for signature in signatures {
            if signature.ignored {
                continue;
            }
            if signature.overloads.is_empty() {
                if signature.implicit
                    && !matches!(
                        untyped_call_policy,
                        functions::UntypedCallPolicy::WithProtocol
                    )
                {
                    self.emit(
                        "no-untyped-call",
                        format!(
                            "Call to untyped function {} in typed context",
                            signature
                                .untyped_call_target()
                                .unwrap_or_else(|| "(unknown)".into())
                        ),
                        call,
                    );
                }
                self.bind_call(call, &signature);
            } else {
                self.check_overload_call(call, &signature);
            }
        }
        true
    }
}
