//! Predicate declarations return bool and retain their branch refinement type.
use super::*;
use crate::infrastructure::mypy::signatures::NarrowingKind;

impl Context {
    pub(in crate::infrastructure::mypy) fn predicate_annotation(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<(NarrowingKind, Type)> {
        if let Expr::StringLiteral(string) = expression {
            let source = format!("({})\n", string.value.to_str());
            let document = mamba_language::application::quality_syntax::parse(
                &source,
                mamba_language::application::quality_syntax::PythonTarget::Py314,
                false,
            )
            .ok()?;
            if !document.is_valid() {
                return None;
            }
            let [mamba_language::application::quality_syntax::python_ast::Stmt::Expr(statement)] =
                document.parsed.syntax().body.as_slice()
            else {
                return None;
            };
            if matches!(statement.value.as_ref(), Expr::StringLiteral(_)) {
                return None;
            }
            return self.predicate_annotation(&statement.value, variables);
        }
        let Expr::Subscript(subscript) = expression else {
            return None;
        };
        let kind = match self.typing_form(&subscript.value, variables).as_deref()? {
            "TypeGuard" => NarrowingKind::TypeGuard,
            "TypeIs" => NarrowingKind::TypeIs,
            _ => return None,
        };
        let ty = if self.predicate_argument_count(&subscript.slice) == 1 {
            self.annotation_with_variables(&subscript.slice, variables)?
        } else {
            Type::Any
        };
        Some((kind, ty))
    }

    pub(in crate::infrastructure::mypy) fn predicate_annotation_error(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<String> {
        if let Expr::StringLiteral(string) = expression {
            let source = format!("({})\n", string.value.to_str());
            let document = mamba_language::application::quality_syntax::parse(
                &source,
                mamba_language::application::quality_syntax::PythonTarget::Py314,
                false,
            )
            .ok()?;
            let [mamba_language::application::quality_syntax::python_ast::Stmt::Expr(statement)] =
                document.parsed.syntax().body.as_slice()
            else {
                return None;
            };
            if !document.is_valid() || matches!(statement.value.as_ref(), Expr::StringLiteral(_)) {
                return None;
            }
            return self.predicate_annotation_error(&statement.value, variables);
        }
        let Expr::Subscript(subscript) = expression else {
            return None;
        };
        let name = self.typing_form(&subscript.value, variables)?;
        (matches!(name.as_str(), "TypeGuard" | "TypeIs")
            && self.predicate_argument_count(&subscript.slice) != 1)
            .then(|| format!("{name} must have exactly one type argument"))
    }

    fn predicate_argument_count(&self, expression: &Expr) -> usize {
        if let Expr::Tuple(tuple) = expression {
            tuple.elts.len()
        } else {
            1
        }
    }

    pub(in crate::infrastructure::mypy) fn predicate_intersection(
        &self,
        input: Type,
        target: Type,
        positive: bool,
        module: &str,
    ) -> Type {
        if input == Type::Never {
            return input;
        }
        if target.is_any() {
            return if positive { target } else { input };
        }
        if self
            .node(input)
            .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
        {
            return input;
        }
        if let Some(Node::Union(items)) = self.node(input).as_deref() {
            return self.union(
                items
                    .iter()
                    .map(|&item| self.predicate_intersection(item, target, positive, module))
                    .collect(),
            );
        }
        if let Some(Node::Union(items)) = self.node(target).as_deref() {
            return if positive {
                self.union(
                    items
                        .iter()
                        .map(|&item| self.predicate_intersection(input, item, true, module))
                        .collect(),
                )
            } else {
                items.iter().fold(input, |input, &item| {
                    self.predicate_intersection(input, item, false, module)
                })
            };
        }
        if matches!(input, Type::Any | Type::AnySpecial | Type::Object) {
            return if positive { target } else { input };
        }
        if let (Some(Node::Container(left, items)), Some(Node::Container(right, _))) =
            (self.node(input).as_deref(), self.node(target).as_deref())
        {
            if left == right && items.iter().any(|ty| ty.is_any()) && self.accepts(input, target) {
                return if positive { target } else { input };
            }
        }
        if self.accepts(target, input) {
            return if positive { input } else { Type::Never };
        }
        if positive && self.accepts(input, target) {
            return target;
        }
        if positive {
            if let (
                Some(Node::Container(left, left_items)),
                Some(Node::Container(right, right_items)),
            ) = (self.node(input).as_deref(), self.node(target).as_deref())
            {
                if left == right {
                    return if left_items.iter().zip(right_items).all(|(&left, &right)| {
                        self.accepts(left, right) || self.accepts(right, left)
                    }) {
                        target
                    } else {
                        Type::Never
                    };
                }
            }
        }
        if positive {
            self.nominal_intersection(input, target, module)
                .unwrap_or(Type::Never)
        } else {
            input
        }
    }

    pub(in crate::infrastructure::mypy) fn callable_partition(
        &self,
        input: Type,
        module: &str,
        positive: bool,
    ) -> Type {
        if input.is_any() {
            return input;
        }
        if self
            .node(input)
            .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
        {
            return input;
        }
        if let Some(Node::Union(items)) = self.node(input).as_deref() {
            return self.union(
                items
                    .iter()
                    .map(|&item| {
                        if item.is_any() || self.callable_signature(item).is_some() == positive {
                            item
                        } else {
                            Type::Never
                        }
                    })
                    .collect(),
            );
        }
        if self.callable_signature(input).is_some() {
            return if positive { input } else { Type::Never };
        }
        if input == Type::None {
            return if positive { Type::Never } else { input };
        }
        if !positive || input == Type::Never {
            return input;
        }
        let name = format!("<callable subtype of {}>", self.name(input));
        let id = self.declare_class(&format!("{module}.{name}"), &name, 0);
        let mut class = (*self.class(id)).clone();
        if !class.methods.contains_key("__call__") {
            if let Some(parent) = self.class_id(input) {
                class.bases = vec![parent];
                class.base_types = vec![input];
            } else {
                class.scalar_bases = vec![input];
            }
            let mut signature = (*self
                .callable_signature(self.bare_callable(&Variables::new()))
                .unwrap())
            .clone();
            let mut parameters = vec![crate::infrastructure::mypy::signatures::Parameter {
                name: "self".into(),
                ty: Some(class.instance),
                kind: crate::infrastructure::mypy::signatures::Kind::PositionalOnly,
                required: true,
            }];
            parameters.extend(signature.parameters.iter().cloned());
            signature.parameters = parameters.into();
            signature.name = Some("__call__".into());
            class.methods.insert(
                "__call__".into(),
                crate::infrastructure::mypy::classes::Method {
                    signature: std::sync::Arc::new(signature),
                    kind: crate::infrastructure::mypy::classes::MethodKind::Instance,
                    setter: None,
                    final_method: false,
                    abstractness: crate::infrastructure::mypy::classes::Abstractness::Concrete,
                },
            );
            self.replace_class(id, class);
        }
        self.class(id).instance
    }
}
