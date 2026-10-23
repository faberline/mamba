//! Standard field defaults retain their inferred result and generic context.
use super::super::types::{Container, Node};
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCall;

impl Scopes {
    pub(in crate::infrastructure::mypy) fn is_dataclass_field(&self, call: &ExprCall) -> bool {
        self.context
            .typing_form(&call.func, &self.variables())
            .as_deref()
            == Some("field")
    }

    pub(in crate::infrastructure::mypy) fn field_context(&self, expected: Type) -> bool {
        self.context.node(self.context.expanded_alias_instance(expected)).is_some_and(|node| {
            matches!(node.as_ref(), Node::Container(_, _) | Node::VariadicTuple(_))
                || matches!(node.as_ref(), Node::AppliedClass { arguments, .. } if !arguments.is_empty())
        })
    }

    pub(in crate::infrastructure::mypy) fn field_default_type(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        expected: Option<Type>,
    ) -> Option<Type> {
        if let Expr::List(list) = expression {
            if list.elts.is_empty() {
                if let Some(expected) = expected.filter(|&expected| {
                    self.context.node(expected).is_some_and(|node| {
                        matches!(node.as_ref(), Node::Container(Container::List, _))
                    })
                }) {
                    return Some(expected);
                }
                return Some(
                    self.context
                        .intern(Node::Container(Container::List, vec![Type::Never])),
                );
            }
        }
        self.context
            .infer_with_calls(expression, environment, &|call| {
                self.expression_type(call, environment)
            })
    }

    pub(in crate::infrastructure::mypy) fn field_factory_type(
        &self,
        expression: &Expr,
        environment: &BTreeMap<String, Type>,
        expected: Option<Type>,
    ) -> Option<(Type, Type)> {
        if let Expr::Name(name) = expression {
            if !self.has_binding(name.id.as_str()) {
                let scalar = match name.id.as_str() {
                    "int" => Some(Type::Int),
                    "str" => Some(Type::Str),
                    "bool" => Some(Type::Bool),
                    "float" => Some(Type::Float),
                    "complex" => Some(Type::Complex),
                    "bytes" => Some(Type::Bytes),
                    _ => None,
                };
                if let Some(scalar) = scalar {
                    return Some((
                        scalar,
                        self.context
                            .class(self.context.builtin_class(scalar))
                            .object,
                    ));
                }
                if let Some(result) = self.empty_field_factory(name.id.as_str(), expected) {
                    let mut signature = Signature::empty(name.id.to_string(), result);
                    signature.constructor = false;
                    signature.name = None;
                    return Some((result, self.context.callable_type(Arc::new(signature))));
                }
            }
        }
        let signature = self.callable_expression_in(expression, environment)?;
        Some((signature.returns?, self.context.callable_type(signature)))
    }

    pub(super) fn dataclass_field_return(
        &self,
        call: &ExprCall,
        environment: &BTreeMap<String, Type>,
        expected: Option<Type>,
    ) -> Option<Type> {
        if !self.is_dataclass_field(call) {
            return None;
        }
        let context = expected.filter(|&ty| self.field_context(ty));
        for keyword in &call.arguments.keywords {
            match keyword.arg.as_ref().map(|name| name.as_str()) {
                Some("default") => {
                    return context
                        .or_else(|| self.field_default_type(&keyword.value, environment, None))
                }
                Some("default_factory") => {
                    return context.or_else(|| {
                        self.field_factory_type(&keyword.value, environment, None)
                            .map(|(returns, _)| returns)
                    })
                }
                _ => {}
            }
        }
        Some(Type::Any)
    }
}
