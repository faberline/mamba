//! Class type parameters retain stable binders across module refreshes.
use super::super::types::{Node, Variables};
use super::*;
use mamba_language::application::quality_syntax::python_ast::{
    Expr, StmtClassDef, StmtFunctionDef, TypeParam,
};

impl Context {
    // Generic is declaration syntax only after its complete binder list is verified.
    pub(super) fn verified_generic_base(
        &self,
        base: &Expr,
        statement: &StmtClassDef,
        variables: &Variables,
        generic: &[Type],
    ) -> bool {
        let Expr::Subscript(subscript) = base else {
            return false;
        };
        if statement.type_params.is_some()
            || generic.is_empty()
            || self.typing_form(&subscript.value, variables).as_deref() != Some("Generic")
        {
            return false;
        }
        let arguments: Vec<_> = match subscript.slice.as_ref() {
            Expr::Tuple(tuple) => tuple.elts.iter().collect(),
            argument => vec![argument],
        };
        let Some(parameters) = arguments
            .into_iter()
            .map(|argument| self.annotation_with_variables(argument, variables))
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        if parameters.as_slice() != generic
            || parameters
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != parameters.len()
            || parameters.iter().any(|&ty| {
                !self.node(ty).is_some_and(|node| {
                    matches!(node.as_ref(), Node::Variable(variable)
                        if !variable.parameter_spec && !variable.type_var_tuple)
                })
            })
        {
            return false;
        }
        let Some(bases) = &statement.arguments else {
            return false;
        };
        bases
            .args
            .iter()
            .filter(|other| other.start() != base.start())
            .all(|other| {
                let Some(ty) = self.class_base_annotation(other, variables) else {
                    return false;
                };
                let mut used = Vec::new();
                self.collect_variables(ty, &mut used);
                used.iter().all(|variable| generic.contains(variable))
            })
    }

    pub(in crate::infrastructure::mypy) fn class_variables(
        &self,
        id: u32,
        statement: &StmtClassDef,
        inherited: &Variables,
    ) -> (Variables, Vec<Type>) {
        let mut variables = inherited.clone();
        let mut generic = Vec::new();
        if let Some(parameters) = &statement.type_params {
            for parameter in &parameters.type_params {
                if let TypeParam::TypeVarTuple(parameter) = parameter {
                    let ty =
                        self.class(id)
                            .generic
                            .get(generic.len())
                            .copied()
                            .filter(|&ty| self.is_tuple_variable(ty))
                            .unwrap_or_else(|| {
                                let default = parameter.default.as_deref().and_then(|value| {
                                    self.tuple_variable_default(value, &variables)
                                });
                                self.tuple_variable(parameter.name.to_string(), default)
                            });
                    variables.insert(parameter.name.to_string(), ty);
                    generic.push(ty);
                    continue;
                }
                if let TypeParam::ParamSpec(parameter) = parameter {
                    let ty =
                        self.class(id)
                            .generic
                            .get(generic.len())
                            .copied()
                            .filter(|&ty| self.is_parameter_spec(ty))
                            .unwrap_or_else(|| {
                                let default = parameter.default.as_deref().and_then(|value| {
                                    self.parameter_spec_default(value, &variables)
                                });
                                self.parameter_spec(parameter.name.to_string(), default)
                            });
                    variables.insert(parameter.name.to_string(), ty);
                    generic.push(ty);
                    continue;
                }
                let TypeParam::TypeVar(parameter) = parameter else {
                    continue;
                };
                let previous = self.class(id).generic.get(generic.len()).copied();
                let ty = previous.filter(|&ty| self.node(ty).is_some_and(|node| matches!(node.as_ref(), Node::Variable(variable) if variable.name == parameter.name.as_str()))).unwrap_or_else(|| {
                    let (bound, values) = match parameter.bound.as_deref() {
                        Some(Expr::Tuple(tuple)) => (Type::Object, tuple.elts.iter().filter_map(|item| self.annotation_with_variables(item, &variables)).collect()),
                        Some(bound) => (self.annotation_with_variables(bound, &variables).unwrap_or(Type::Object), Vec::new()),
                        None => (Type::Object, Vec::new()),
                    };
                    let default = parameter.default.as_deref().and_then(|value| self.annotation_with_variables(value, &variables));
                    self.variable_with_variance(parameter.name.to_string(), bound, values, default, super::super::types::ParameterVariance::Inferred)
                });
                variables.insert(parameter.name.to_string(), ty);
                generic.push(ty);
            }
        } else if let Some(arguments) = &statement.arguments {
            let explicit = arguments.args.iter().any(|base| matches!(base, Expr::Subscript(subscript) if matches!(self.typing_form(&subscript.value, &variables).as_deref(), Some("Generic" | "Protocol"))));
            for base in &arguments.args {
                let Expr::Subscript(base) = base else {
                    continue;
                };
                if !matches!(
                    self.typing_form(&base.value, &variables).as_deref(),
                    Some("Generic" | "Protocol")
                ) {
                    if !explicit {
                        if let Some(ty) = self
                            .annotation_with_variables(&Expr::Subscript(base.clone()), &variables)
                        {
                            self.collect_variables(ty, &mut generic);
                        }
                    }
                    continue;
                }
                let items: Vec<_> = match base.slice.as_ref() {
                    Expr::Tuple(tuple) => tuple.elts.iter().collect(),
                    expression => vec![expression],
                };
                for item in items {
                    if let Some(ty) = self
                        .annotation_with_variables(item, &variables)
                        .map(|ty| self.unpack_target(ty).unwrap_or(ty))
                        .filter(|&ty| {
                            self.node(ty)
                                .is_some_and(|node| matches!(node.as_ref(), Node::Variable(_)))
                        })
                    {
                        if !generic.contains(&ty) {
                            generic.push(ty);
                        }
                    }
                }
            }
        }
        let bound = self.applied_class(id, generic.clone(), false);
        self.bind_source_self(id, bound, &mut variables);
        (variables, generic)
    }

    fn bind_source_self(&self, id: u32, bound: Type, variables: &mut Variables) {
        let names: Vec<_> = variables
            .iter()
            .filter(|(_, ty)| {
                matches!(self.node(**ty).as_deref(), Some(Node::TypingForm(form)) if form == "Self")
                    || self.is_source_self_variable(**ty)
            })
            .map(|(name, _)| name.clone())
            .collect();
        if names.is_empty() {
            return;
        }
        // Class IDs already separate projects, targets and lexical declarations.
        let variable = self.native_variable("source-class-Self", id, "Self".into(), bound);
        for name in names {
            variables.insert(name, variable);
        }
    }

    pub(in crate::infrastructure::mypy) fn is_source_self_variable(&self, ty: Type) -> bool {
        let Some(node) = self.node(ty) else {
            return false;
        };
        let Node::Variable(variable) = node.as_ref() else {
            return false;
        };
        let Some(bound) = self.node(variable.bound) else {
            return false;
        };
        let owner = match bound.as_ref() {
            Node::Class(id) | Node::Instance(id) | Node::AppliedClass { id, .. } => *id,
            _ => return false,
        };
        self.native_variable_identity("source-class-Self", owner) == Some(variable.id)
    }

    pub(in crate::infrastructure::mypy) fn source_self_variable(&self, id: u32) -> Option<Type> {
        self.native_variable_identity("source-class-Self", id)?;
        let bound = self.applied_class(id, self.class(id).generic.clone(), false);
        Some(self.native_variable("source-class-Self", id, "Self".into(), bound))
    }

    pub(in crate::infrastructure::mypy) fn bind_source_self_receiver(
        &self,
        signature: &mut Signature,
        function: &StmtFunctionDef,
        kind: MethodKind,
    ) {
        let allocation = function.name.as_str() == "__new__";
        if kind == MethodKind::Static && !allocation {
            return;
        }
        if !function
            .parameters
            .iter()
            .next()
            .is_some_and(|parameter| parameter.annotation().is_none())
        {
            return;
        }
        let Some(variable) = signature
            .generic
            .iter()
            .copied()
            .find(|&ty| self.is_source_self_variable(ty))
        else {
            return;
        };
        let mut parameters = signature.parameters.to_vec();
        let Some(first) = parameters.first_mut().filter(|first| first.ty.is_none()) else {
            return;
        };
        first.ty = Some(if kind == MethodKind::Class || allocation {
            self.intern(Node::Container(
                super::super::types::Container::TypeObject,
                vec![variable],
            ))
        } else {
            variable
        });
        signature.parameters = parameters.into();
    }

    pub(in crate::infrastructure::mypy) fn class_scope_variables(
        &self,
        id: u32,
        mut variables: Variables,
    ) -> Variables {
        for &ty in &self.class(id).generic {
            if let Some(node) = self.node(ty) {
                if let Node::Variable(variable) = node.as_ref() {
                    variables.insert(variable.name.clone(), ty);
                }
            }
        }
        let bound = self.applied_class(id, self.class(id).generic.clone(), false);
        self.bind_source_self(id, bound, &mut variables);
        variables
    }
}
