//! Protocol declarations and runtime checks retain their original diagnostics.
use super::*;
use quality_syntax::python_ast::{ExprCall, StmtClassDef, StmtFunctionDef};

fn quoted_list(names: impl Iterator<Item = String>) -> String {
    let names: Vec<_> = names.map(|name| format!("\"{name}\"")).collect();
    match names.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, earlier)) => format!("{} and {last}", earlier.join(", ")),
        None => String::new(),
    }
}

impl Checker<'_> {
    pub(super) fn check_protocol_declaration(&mut self, statement: &StmtClassDef) {
        let Some(class) = self
            .scopes
            .class_object(statement.name.as_str())
            .and_then(|ty| self.context.class_id(ty))
            .map(|id| self.context.class(id))
        else {
            return;
        };
        let prefix = &self.source.text[..statement.name.start().to_usize()];
        let start = ruff_text_size::TextSize::try_from(
            prefix
                .rfind("class ")
                .unwrap_or(statement.start().to_usize()),
        )
        .expect("class offset fits in u32");
        if class.protocol {
            for (index, &parameter) in class.generic.iter().enumerate() {
                let Some(node) = self.context.node(parameter) else {
                    continue;
                };
                let types::Node::Variable(variable) = node.as_ref() else {
                    continue;
                };
                if variable.variance == types::ParameterVariance::Inferred {
                    continue;
                }
                let expected = self.context.protocol_parameter_variance(
                    self.context.class_id(class.instance).unwrap(),
                    index,
                );
                if variable.variance != expected {
                    let declared = match variable.variance {
                        types::ParameterVariance::Covariant => "Covariant",
                        types::ParameterVariance::Contravariant => "Contravariant",
                        _ => "Invariant",
                    };
                    let expected = match expected {
                        types::ParameterVariance::Covariant => "covariant",
                        types::ParameterVariance::Contravariant => "contravariant",
                        _ => "invariant",
                    };
                    self.emit("misc", format!("{declared} type variable \"{}\" used in protocol where {expected} one is expected", variable.name), &ruff_text_size::TextRange::new(start, statement.end()));
                }
            }
        }
        if class.runtime_protocol && !class.protocol {
            self.emit(
                "misc",
                "@runtime_checkable can only be used with protocol classes".into(),
                &ruff_text_size::TextRange::new(start, statement.end()),
            );
        }
        if class.protocol
            && class.bases.iter().any(|&id| {
                !self.context.class(id).protocol && self.context.class(id).name != "object"
            })
        {
            self.emit(
                "misc",
                "All bases of a protocol must be protocols".into(),
                &ruff_text_size::TextRange::at(start, ruff_text_size::TextSize::new(1)),
            );
        }
    }

    fn check_abstract_call(&mut self, id: u32, call: &ExprCall) {
        let attributes = self.context.abstract_members(id);
        if attributes.is_empty() {
            return;
        }
        let implicit: Vec<_> = attributes
            .iter()
            .filter(|&(_, &implicit)| implicit)
            .map(|(name, _)| name.clone())
            .collect();
        let hint = match implicit.as_slice() {
            [] => None,
            [name] => Some(format!("\"{name}\" is implicitly abstract because it has an empty function body. If it is not meant to be abstract, explicitly `return` or `return None`.")),
            _ => Some(format!("The following methods were marked implicitly abstract because they have empty function bodies: {}. If they are not meant to be abstract, explicitly `return` or `return None`.", quoted_list(implicit.into_iter()))),
        };
        self.emit_with_hint(
            "abstract",
            format!(
                "Cannot instantiate abstract class \"{}\" with abstract attribute{} {}",
                self.context.class(id).name,
                if attributes.len() == 1 { "" } else { "s" },
                quoted_list(attributes.into_keys())
            ),
            call,
            hint,
        );
    }
    pub(super) fn protocol_stub(&self, function: &StmtFunctionDef) -> bool {
        self.in_class && self.class_types.last().copied().flatten().and_then(|ty| self.context.class_id(ty)).is_some_and(|id| self.context.class(id).protocol)
            && function.body.iter().all(|statement| matches!(statement, Stmt::Pass(_)) || matches!(statement, Stmt::Expr(expression) if matches!(expression.value.as_ref(), Expr::EllipsisLiteral(_) | Expr::StringLiteral(_))))
    }

    fn runtime_protocol_targets(&self, expression: &Expr) -> Vec<u32> {
        if let Expr::Tuple(tuple) = expression {
            return tuple
                .elts
                .iter()
                .flat_map(|item| self.runtime_protocol_targets(item))
                .collect();
        }
        self.infer(expression)
            .filter(|&ty| self.context.is_class_value(ty))
            .and_then(|ty| self.context.class_id(ty))
            .into_iter()
            .collect()
    }

    fn parameterized_runtime_target(&self, expression: &Expr) -> bool {
        if let Expr::Tuple(tuple) = expression {
            return tuple
                .elts
                .iter()
                .any(|item| self.parameterized_runtime_target(item));
        }
        if let Expr::Subscript(subscript) = expression {
            return self.scopes.annotation(expression).is_some()
                && self.infer(&subscript.value).is_some_and(|ty| {
                    self.context.is_class_value(ty)
                        || self
                            .context
                            .callable_signature(ty)
                            .is_some_and(|signature| signature.constructor)
                });
        }
        self.infer(expression).and_then(|ty| self.context.node(ty)).is_some_and(|node| matches!(node.as_ref(), types::Node::AppliedClass { arguments, value: true, .. } if !arguments.is_empty()))
    }

    pub(super) fn check_class_value_call(&mut self, ty: types::Type, call: &ExprCall) -> bool {
        if self.context.is_class_value(ty) {
            if let Some(id) = self.context.class_id(ty) {
                if self.context.class(id).protocol {
                    self.emit(
                        "misc",
                        format!(
                            "Cannot instantiate protocol class \"{}\"",
                            self.context.class(id).name
                        ),
                        call,
                    );
                    return true;
                }
                self.check_abstract_call(id, call);
            }
        }
        false
    }

    pub(super) fn check_protocol_call(&mut self, call: &ExprCall) -> bool {
        if let Some(ty) = self
            .infer(&call.func)
            .filter(|&ty| self.context.is_class_value(ty))
        {
            if self.check_class_value_call(ty, call) {
                return true;
            }
        }
        if let Some(predicate) = self.runtime_predicate_name(&call.func) {
            if let [_, target] = call.arguments.args.as_slice() {
                if matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == predicate)
                    && self.parameterized_runtime_target(target)
                {
                    self.emit(
                        "misc",
                        "Parameterized generics cannot be used with class or instance checks"
                            .into(),
                        call,
                    );
                }
                let mut runtime_error = false;
                for id in self.runtime_protocol_targets(target) {
                    let class = self.context.class(id);
                    if class.protocol && !class.runtime_protocol && !runtime_error {
                        self.emit("misc", "Only @runtime_checkable protocols can be used with instance and class checks".into(), call);
                        runtime_error = true;
                    }
                    if class.protocol && predicate == "issubclass" {
                        let fields: Vec<_> = self
                            .context
                            .protocol_members(id)
                            .into_iter()
                            .filter(|name| {
                                self.context.class_method(class.instance, name).is_none()
                            })
                            .collect();
                        if !fields.is_empty() {
                            self.emit_with_hint("misc", "Only protocols that don't have non-method members can be used with issubclass()".into(), call, Some(format!("Protocol \"{}\" has non-method member(s): {}", class.name, fields.join(", "))));
                        }
                    }
                }
            }
        }
        false
    }
}
