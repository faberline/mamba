//! Imported bounds refresh declarations without changing their binder identity.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{Stmt, StmtFunctionDef, TypeParam};

impl Context {
    pub(in crate::infrastructure::mypy) fn refresh_function_variables(
        &self,
        function: &StmtFunctionDef,
        namespace: &Variables,
        previous: &Variables,
    ) -> Variables {
        let mut variables = namespace.clone();
        for (name, &ty) in previous {
            if !variables.contains_key(name)
                && self
                    .node(ty)
                    .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
            {
                variables.insert(name.clone(), ty);
            }
        }
        for parameter in function
            .type_params
            .iter()
            .flat_map(|parameters| &parameters.type_params)
        {
            let special_name = match parameter {
                TypeParam::ParamSpec(parameter) => Some(parameter.name.as_str()),
                TypeParam::TypeVarTuple(parameter) => Some(parameter.name.as_str()),
                _ => None,
            };
            if let Some(name) = special_name {
                if let Some(&ty) = previous.get(name) {
                    variables.insert(name.to_owned(), ty);
                }
                continue;
            }
            let TypeParam::TypeVar(parameter) = parameter else {
                continue;
            };
            let (bound, values) = match parameter.bound.as_deref() {
                Some(Expr::Tuple(tuple)) => {
                    let Some(values) = tuple
                        .elts
                        .iter()
                        .map(|value| self.annotation_with_variables(value, &variables))
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };
                    (Type::Object, values)
                }
                Some(bound) => {
                    let Some(bound) = self.annotation_with_variables(bound, &variables) else {
                        continue;
                    };
                    (bound, Vec::new())
                }
                None => (Type::Object, Vec::new()),
            };
            let default = parameter
                .default
                .as_deref()
                .and_then(|value| self.annotation_with_variables(value, &variables));
            let old = previous
                .get(parameter.name.as_str())
                .copied()
                .filter(|old| Some(old) != namespace.get(parameter.name.as_str()))
                .and_then(|ty| self.node(ty));
            let ty = if let Some(Node::Variable(old)) = old.as_deref() {
                self.intern(Node::Variable(variables::Variable {
                    bound,
                    values,
                    default,
                    ..old.clone()
                }))
            } else {
                self.source_type_variable(
                    &parameter.name,
                    "function.TypeVar",
                    parameter.name.to_string(),
                    bound,
                    values,
                    default,
                    ParameterVariance::Invariant,
                )
            };
            variables.insert(parameter.name.to_string(), ty);
        }
        variables
    }

    pub(in crate::infrastructure::mypy) fn refresh_legacy_variables(
        &self,
        body: &[Stmt],
        variables: &mut Variables,
    ) {
        for statement in body {
            let Stmt::Assign(assignment) = statement else {
                continue;
            };
            let ([Expr::Name(name)], Expr::Call(call)) =
                (assignment.targets.as_slice(), assignment.value.as_ref())
            else {
                continue;
            };
            if !matches!(call.func.as_ref(), Expr::Name(name) if name.id.as_str() == "TypeVar")
                && !matches!(call.func.as_ref(), Expr::Attribute(attribute) if attribute.attr.as_str() == "TypeVar")
            {
                continue;
            }
            let Some(values) = call
                .arguments
                .args
                .iter()
                .skip(1)
                .map(|value| self.annotation_with_variables(value, variables))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let mut bound = Type::Object;
            let mut default = None;
            let mut known = true;
            for keyword in &call.arguments.keywords {
                match keyword.arg.as_ref().map(|name| name.as_str()) {
                    Some("bound") => {
                        match self.annotation_with_variables(&keyword.value, variables) {
                            Some(ty) => bound = ty,
                            None => known = false,
                        }
                    }
                    Some("default") => {
                        default = self.annotation_with_variables(&keyword.value, variables)
                    }
                    _ => {}
                }
            }
            if !known {
                continue;
            }
            let old = variables
                .get(name.id.as_str())
                .and_then(|&ty| self.node(ty));
            if let Some(Node::Variable(old)) = old.as_deref() {
                if !old.parameter_spec && !old.type_var_tuple {
                    variables.insert(
                        name.id.to_string(),
                        self.intern(Node::Variable(variables::Variable {
                            bound,
                            values,
                            default,
                            ..old.clone()
                        })),
                    );
                }
            } else {
                self.legacy_variables(std::slice::from_ref(statement), variables);
            }
        }
    }
}
