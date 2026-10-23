//! Variadic tuple parameters retain stable declaration identities and defaults.
use super::*;
use mamba_language::application::quality_syntax::python_ast::ExprCall;

impl Context {
    pub(in crate::infrastructure::mypy) fn tuple_variable(
        &self,
        name: String,
        default: Option<Type>,
    ) -> Type {
        self.intern(Node::Variable(variables::Variable {
            id: self.variable_id(),
            name,
            bound: Type::Object,
            values: Vec::new(),
            default,
            variance: ParameterVariance::Invariant,
            parameter_spec: false,
            type_var_tuple: true,
        }))
    }

    pub(in crate::infrastructure::mypy) fn tuple_variable_default(
        &self,
        expression: &Expr,
        variables: &VariableMap,
    ) -> Option<Type> {
        let ty = self
            .annotation_with_variables(expression, variables)
            .unwrap_or(Type::Any);
        Some(if let Some(target) = self.unpack_target(ty) {
            if self.is_tuple_variable(target)
                || self.node(target).is_some_and(|node| {
                    matches!(node.as_ref(), Node::Tuple(_) | Node::VariadicTuple(_))
                })
            {
                target
            } else {
                self.intern(Node::VariadicTuple(Type::Any))
            }
        } else {
            self.intern(Node::Tuple(vec![Type::Any]))
        })
    }

    pub(in crate::infrastructure::mypy) fn legacy_type_var_tuple(
        &self,
        name: &str,
        call: &ExprCall,
        variables: &mut Variables,
    ) -> bool {
        if self.typing_form(&call.func, variables).as_deref() != Some("TypeVarTuple") {
            return false;
        }
        let default = call
            .arguments
            .keywords
            .iter()
            .find(|keyword| {
                keyword
                    .arg
                    .as_ref()
                    .is_some_and(|name| name.as_str() == "default")
            })
            .and_then(|keyword| self.tuple_variable_default(&keyword.value, variables));
        variables.insert(
            name.to_string(),
            self.source_tuple_variable(call, "legacy.TypeVarTuple", name.to_string(), default),
        );
        true
    }
}
