//! Type-variable declarations keep lexical identity, bounds, and substitutions.
use super::*;
use mamba_language::application::quality_syntax::python_ast::{Stmt, StmtFunctionDef, TypeParam};

mod snapshot;
pub(crate) use snapshot::{VariableMap, Variables};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::infrastructure::mypy) enum ParameterVariance {
    Invariant,
    Covariant,
    Contravariant,
    Inferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Variable {
    pub id: u32,
    pub name: String,
    pub bound: Type,
    pub values: Vec<Type>,
    pub default: Option<Type>,
    pub variance: ParameterVariance,
    pub parameter_spec: bool,
    pub type_var_tuple: bool,
}

impl Context {
    pub(crate) fn legacy_variables(&self, body: &[Stmt], variables: &mut Variables) {
        for statement in body {
            let Stmt::Assign(assignment) = statement else {
                continue;
            };
            let [Expr::Name(name)] = assignment.targets.as_slice() else {
                continue;
            };
            let Expr::Call(call) = assignment.value.as_ref() else {
                continue;
            };
            let constructor = match call.func.as_ref() {
                Expr::Name(name) => name.id.as_str(),
                Expr::Attribute(attribute) => attribute.attr.as_str(),
                _ => continue,
            };
            if self.legacy_type_var_tuple(name.id.as_str(), call, variables) {
                continue;
            }
            if constructor == "ParamSpec"
                || self.typing_form(&call.func, variables).as_deref() == Some("ParamSpec")
            {
                if !matches!(call.arguments.args.first(), Some(Expr::StringLiteral(string)) if string.value.to_str() == name.id.as_str())
                {
                    continue;
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
                    .and_then(|keyword| self.parameter_spec_default(&keyword.value, variables));
                variables.insert(
                    name.id.to_string(),
                    self.source_parameter_spec(
                        name,
                        "legacy.ParamSpec",
                        name.id.to_string(),
                        default,
                    ),
                );
                continue;
            }
            if constructor != "TypeVar" {
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
            let mut variance = ParameterVariance::Invariant;
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
                    Some("covariant") if matches!(&keyword.value, Expr::BooleanLiteral(value) if value.value) => {
                        variance = ParameterVariance::Covariant
                    }
                    Some("contravariant") if matches!(&keyword.value, Expr::BooleanLiteral(value) if value.value) => {
                        variance = ParameterVariance::Contravariant
                    }
                    Some("infer_variance") if matches!(&keyword.value, Expr::BooleanLiteral(value) if value.value) => {
                        variance = ParameterVariance::Inferred
                    }
                    _ => {}
                }
            }
            if known {
                let ty = self.source_type_variable(
                    name,
                    "legacy.TypeVar",
                    name.id.to_string(),
                    bound,
                    values,
                    default,
                    variance,
                );
                variables.insert(name.id.to_string(), ty);
            }
        }
    }

    pub(crate) fn function_variables(
        &self,
        function: &StmtFunctionDef,
        inherited: &Variables,
    ) -> Variables {
        let mut variables = inherited.clone();
        if let Some(parameters) = &function.type_params {
            for parameter in &parameters.type_params {
                if let TypeParam::TypeVarTuple(parameter) = parameter {
                    let default = parameter
                        .default
                        .as_deref()
                        .and_then(|value| self.tuple_variable_default(value, &variables));
                    variables.insert(
                        parameter.name.to_string(),
                        self.source_tuple_variable(
                            &parameter.name,
                            "function.TypeVarTuple",
                            parameter.name.to_string(),
                            default,
                        ),
                    );
                    continue;
                }
                if let TypeParam::ParamSpec(parameter) = parameter {
                    let default = parameter
                        .default
                        .as_deref()
                        .and_then(|value| self.parameter_spec_default(value, &variables));
                    variables.insert(
                        parameter.name.to_string(),
                        self.source_parameter_spec(
                            &parameter.name,
                            "function.ParamSpec",
                            parameter.name.to_string(),
                            default,
                        ),
                    );
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
                let ty = self.source_type_variable(
                    &parameter.name,
                    "function.TypeVar",
                    parameter.name.to_string(),
                    bound,
                    values,
                    default,
                    ParameterVariance::Invariant,
                );
                variables.insert(parameter.name.to_string(), ty);
            }
        }
        variables
    }

    pub(in crate::infrastructure::mypy) fn source_type_variable(
        &self,
        declaration: &impl ruff_text_size::Ranged,
        role: &'static str,
        name: String,
        bound: Type,
        values: Vec<Type>,
        default: Option<Type>,
        variance: ParameterVariance,
    ) -> Type {
        let id = self.source_variable_id(declaration, role, &name);
        let ty = self.intern(Node::Variable(Variable {
            id,
            name,
            bound,
            values,
            default,
            variance,
            parameter_spec: false,
            type_var_tuple: false,
        }));
        self.record_returned_callable_variable(declaration, id, role);
        ty
    }

    pub(in crate::infrastructure::mypy) fn source_parameter_spec(
        &self,
        declaration: &impl ruff_text_size::Ranged,
        role: &'static str,
        name: String,
        default: Option<Type>,
    ) -> Type {
        self.intern(Node::Variable(Variable {
            id: self.source_variable_id(declaration, role, &name),
            name,
            bound: Type::Object,
            values: Vec::new(),
            default,
            variance: ParameterVariance::Invariant,
            parameter_spec: true,
            type_var_tuple: false,
        }))
    }

    pub(in crate::infrastructure::mypy) fn source_tuple_variable(
        &self,
        declaration: &impl ruff_text_size::Ranged,
        role: &'static str,
        name: String,
        default: Option<Type>,
    ) -> Type {
        self.intern(Node::Variable(Variable {
            id: self.source_variable_id(declaration, role, &name),
            name,
            bound: Type::Object,
            values: Vec::new(),
            default,
            variance: ParameterVariance::Invariant,
            parameter_spec: false,
            type_var_tuple: true,
        }))
    }

    pub(in crate::infrastructure::mypy) fn variable(
        &self,
        name: String,
        bound: Type,
        values: Vec<Type>,
        default: Option<Type>,
    ) -> Type {
        self.variable_with_variance(name, bound, values, default, ParameterVariance::Invariant)
    }

    pub(in crate::infrastructure::mypy) fn variable_with_variance(
        &self,
        name: String,
        bound: Type,
        values: Vec<Type>,
        default: Option<Type>,
        variance: ParameterVariance,
    ) -> Type {
        self.intern(Node::Variable(Variable {
            id: self.variable_id(),
            name,
            bound,
            values,
            default,
            variance,
            parameter_spec: false,
            type_var_tuple: false,
        }))
    }

    pub(crate) fn substitute(&self, ty: Type, solutions: &BTreeMap<Type, Type>) -> Type {
        if let Some(&replacement) = solutions.get(&ty) {
            return replacement;
        }
        let Some(node) = self.node(ty) else { return ty };
        let substituted = match node.as_ref() {
            Node::Container(kind, args) => Node::Container(
                *kind,
                args.iter()
                    .map(|&arg| self.substitute(arg, solutions))
                    .collect(),
            ),
            Node::Tuple(args) => {
                return self.normalize_tuple(
                    args.iter()
                        .map(|&arg| self.substitute(arg, solutions))
                        .collect(),
                )
            }
            Node::VariadicTuple(item) => Node::VariadicTuple(self.substitute(*item, solutions)),
            Node::Unpack(item) => Node::Unpack(self.substitute(*item, solutions)),
            Node::ParamSpecArgs(variable) => {
                Node::ParamSpecArgs(self.substitute(*variable, solutions))
            }
            Node::ParamSpecKwargs(variable) => {
                Node::ParamSpecKwargs(self.substitute(*variable, solutions))
            }
            Node::Parameters(parameters, ellipsis) => Node::Parameters(
                parameters
                    .iter()
                    .map(|parameter| super::super::signatures::Parameter {
                        ty: parameter.ty.map(|ty| self.substitute(ty, solutions)),
                        ..parameter.clone()
                    })
                    .collect(),
                *ellipsis,
            ),
            Node::Callable(signature) => Node::Callable(std::sync::Arc::new(
                self.substitute_callable(signature, solutions),
            )),
            Node::AppliedClass {
                id,
                arguments,
                value,
            } => Node::AppliedClass {
                id: *id,
                arguments: arguments
                    .iter()
                    .map(|&argument| self.substitute(argument, solutions))
                    .collect(),
                value: *value,
            },
            Node::AliasInstance(id, arguments) => Node::AliasInstance(
                *id,
                arguments
                    .iter()
                    .map(|&argument| self.substitute(argument, solutions))
                    .collect(),
            ),
            Node::InferredAlias {
                id,
                arguments,
                target,
            } => Node::InferredAlias {
                id: *id,
                arguments: arguments
                    .iter()
                    .map(|&ty| self.substitute(ty, solutions))
                    .collect(),
                target: self.substitute(*target, solutions),
            },
            Node::Union(items) => {
                return self.union(
                    items
                        .iter()
                        .map(|&item| self.substitute(item, solutions))
                        .collect(),
                )
            }
            Node::Deleted(_)
            | Node::Variable(_)
            | Node::Instance(_)
            | Node::Class(_)
            | Node::Literal(_)
            | Node::LiteralValue(_)
            | Node::LiteralForm
            | Node::TypingForm(_) => return ty,
            Node::Alias(_, _) => return ty,
        };
        self.intern(substituted)
    }

    pub(crate) fn collect_variables(&self, ty: Type, variables: &mut Vec<Type>) {
        let Some(node) = self.node(ty) else { return };
        match node.as_ref() {
            Node::Variable(_) if !variables.contains(&ty) => variables.push(ty),
            Node::Container(_, args)
            | Node::Tuple(args)
            | Node::Union(args)
            | Node::AppliedClass {
                arguments: args, ..
            }
            | Node::AliasInstance(_, args) => {
                for &arg in args {
                    self.collect_variables(arg, variables);
                }
            }
            Node::InferredAlias { arguments, .. } => {
                for &argument in arguments {
                    self.collect_variables(argument, variables);
                }
            }
            Node::VariadicTuple(item) | Node::Unpack(item) => {
                self.collect_variables(*item, variables)
            }
            Node::ParamSpecArgs(variable) | Node::ParamSpecKwargs(variable) => {
                self.collect_variables(*variable, variables)
            }
            Node::Parameters(parameters, _) => {
                for parameter in parameters {
                    if let Some(ty) = parameter.ty {
                        self.collect_variables(ty, variables);
                    }
                }
            }
            Node::Callable(signature) => {
                for parameter in signature.parameters.iter() {
                    if let Some(ty) = parameter.ty {
                        self.collect_variables(ty, variables);
                    }
                }
                if let Some(ty) = signature.returns {
                    self.collect_variables(ty, variables);
                }
                if let Some((_, ty)) = signature.narrowing {
                    self.collect_variables(ty, variables);
                }
            }
            Node::Deleted(_)
            | Node::Variable(_)
            | Node::Instance(_)
            | Node::Class(_)
            | Node::Literal(_)
            | Node::LiteralValue(_)
            | Node::LiteralForm
            | Node::TypingForm(_) => {}
            Node::Alias(_, _) => {}
        }
    }
}
